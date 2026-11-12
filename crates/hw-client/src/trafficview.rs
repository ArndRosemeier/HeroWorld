//! Traffic on screen: the vehicles near the camera (worked out from the town
//! seed and the game time, see `hw_sim::traffic`), one instanced LOD set per
//! model and paint, moved every frame. Car wheels are their own instances,
//! turning with the distance driven.

use engine::prelude::*;
use glam::{DQuat, DVec2, DVec3};
use hw_gen::urban::props::{CarModel, CAR_PAINTS};
use hw_sim::traffic::{Traffic, Vehicle, VehicleKind, BUS_LIVERIES, DWELL};
use std::collections::BTreeMap;

/// Vehicles are shown within this distance of the camera...
pub const SHOW_RADIUS: f64 = 700.0;
/// ...and at most this many at once (the nearest).
pub const MAX_SHOWN: usize = 700;
/// Wheels are turned only on cars this close.
const WHEEL_RADIUS: f64 = 120.0;
/// Bus doors are drawn on their own this close (further off the bus's far
/// model has them shut in its body).
const LEAF_RADIUS: f64 = 150.0;

/// Whose wheels: a car model's, or a bus's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WheelOf {
    Car(CarModel),
    Bus,
}

/// How far a bus's doors stand open after `standing` seconds at a stop:
/// they open as it comes to rest and shut a moment before it leaves.
pub fn doors_open(standing: f64) -> f64 {
    let opening = ((standing - 0.6) / 1.0).clamp(0.0, 1.0);
    let shutting = ((DWELL - 1.0 - standing) / 1.0).clamp(0.0, 1.0);
    let t = opening.min(shutting);
    t * t * (3.0 - 2.0 * t)
}

/// A wrecked car's doors: each one's LOD levels, hinge, axis and how far
/// it opens (see `propview::car_door_levels`).
type DoorSets = Vec<(Vec<LodLevel>, DVec3, DVec3, f64)>;

/// A vehicle model made on the worker thread (see [`Models`]).
enum Made {
    Body(VehicleKind, Vec<LodLevel>),
    Wheel(WheelOf, Vec<LodLevel>),
    /// A bus door leaf: (livery, door).
    Leaf((u8, usize), Vec<LodLevel>),
    Shell(VehicleKind, Vec<LodLevel>),
    Doors(VehicleKind, DoorSets),
}

/// Every vehicle's models, made once on a worker thread when the game
/// starts: a car is 10 to 20 ms of meshing, too much for a frame the
/// first time each model and paint drives into view. Cars whose model is
/// not made yet wait off screen (a moment at the start).
struct Models {
    rx: std::sync::mpsc::Receiver<Made>,
    bodies: BTreeMap<VehicleKind, Vec<LodLevel>>,
    wheels: BTreeMap<WheelOf, Vec<LodLevel>>,
    leaves: BTreeMap<(u8, usize), Vec<LodLevel>>,
    shells: BTreeMap<VehicleKind, Vec<LodLevel>>,
    doors: BTreeMap<VehicleKind, DoorSets>,
}

impl Models {
    fn start() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            engine::parallel::background_thread();
            let cars: Vec<VehicleKind> = [CarModel::Cruiser, CarModel::Bubble, CarModel::Hover]
                .into_iter()
                .flat_map(|m| (0..CAR_PAINTS).map(move |p| VehicleKind::Car(m, p)))
                .collect();
            let mut bodies = cars.clone();
            bodies.push(VehicleKind::Taxi);
            bodies.extend((0..CAR_PAINTS).map(VehicleKind::SkyCar));
            bodies.extend((0..BUS_LIVERIES).map(VehicleKind::Bus));
            for kind in bodies {
                let levels = match kind {
                    VehicleKind::Car(m, paint) => crate::propview::moving_car_levels(m, paint),
                    VehicleKind::SkyCar(paint) => crate::propview::sky_car_levels(paint),
                    VehicleKind::Taxi => crate::propview::taxi_levels(),
                    VehicleKind::Bus(livery) => crate::propview::bus_levels(livery),
                };
                if tx.send(Made::Body(kind, levels)).is_err() {
                    return;
                }
            }
            for m in [CarModel::Cruiser, CarModel::Bubble, CarModel::Hover] {
                if let Some(levels) = crate::propview::wheel_levels(m) {
                    if tx.send(Made::Wheel(WheelOf::Car(m), levels)).is_err() {
                        return;
                    }
                }
            }
            let bus_wheels = crate::propview::bus_wheel_levels();
            if tx.send(Made::Wheel(WheelOf::Bus, bus_wheels)).is_err() {
                return;
            }
            for livery in 0..BUS_LIVERIES {
                for door in 0..crate::carmesh::BUS_DOORS.len() {
                    let levels = crate::propview::bus_leaf_levels(livery, door);
                    if tx.send(Made::Leaf((livery, door), levels)).is_err() {
                        return;
                    }
                }
            }
            // What wrecks need, last.
            for kind in cars {
                let VehicleKind::Car(m, paint) = kind else {
                    continue;
                };
                let shell = Made::Shell(kind, shell_levels(m, paint));
                let doors = Made::Doors(kind, crate::propview::car_door_levels(m, paint));
                if tx.send(shell).is_err() || tx.send(doors).is_err() {
                    return;
                }
            }
        });
        Self {
            rx,
            bodies: BTreeMap::new(),
            wheels: BTreeMap::new(),
            leaves: BTreeMap::new(),
            shells: BTreeMap::new(),
            doors: BTreeMap::new(),
        }
    }

    /// Takes in what the worker finished.
    fn collect(&mut self) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Made::Body(k, l) => {
                    self.bodies.insert(k, l);
                }
                Made::Wheel(k, l) => {
                    self.wheels.insert(k, l);
                }
                Made::Leaf(k, l) => {
                    self.leaves.insert(k, l);
                }
                Made::Shell(k, l) => {
                    self.shells.insert(k, l);
                }
                Made::Doors(k, l) => {
                    self.doors.insert(k, l);
                }
            }
        }
    }
}

/// Spawns the set made for `key` the first time it is wanted (meshes made
/// on the worker; Engine uploads them). `None` while it is not made yet.
fn spawn_made<K: Ord + Copy>(
    world: &mut World,
    spawned: &mut BTreeMap<K, LodId>,
    made: &mut BTreeMap<K, Vec<LodLevel>>,
    key: K,
) -> EngineResult<Option<LodId>> {
    if let Some(id) = spawned.get(&key) {
        return Ok(Some(*id));
    }
    let Some(levels) = made.remove(&key) else {
        return Ok(None);
    };
    let id = world.spawn_lod(levels)?;
    crate::hitch::count("vehicle models uploaded", 1);
    spawned.insert(key, id);
    Ok(Some(id))
}

pub struct TrafficView {
    models: Models,
    bodies: BTreeMap<VehicleKind, LodId>,
    wheels: BTreeMap<WheelOf, LodId>,
    /// Bus door leaves by (livery, door).
    leaves: BTreeMap<(u8, usize), LodId>,
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
    /// The moving vehicles drawn this frame near enough for the people in
    /// them to show (see `riderview`), nearest first.
    riding: Vec<Vehicle>,
}

impl Default for TrafficView {
    fn default() -> Self {
        Self {
            models: Models::start(),
            bodies: BTreeMap::new(),
            wheels: BTreeMap::new(),
            leaves: BTreeMap::new(),
            near: Vec::new(),
            refresh: 0.0,
            wrecks: BTreeMap::new(),
            shells: BTreeMap::new(),
            doors: BTreeMap::new(),
            riding: Vec::new(),
        }
    }
}

impl TrafficView {
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes every vehicle off the streets (another town or square comes).
    /// The models stay: they are the same everywhere, and making them
    /// again took a long frame each time the square moved.
    pub fn clear(&mut self, world: &mut World) {
        let doors = self.doors.values().flatten().map(|d| d.0);
        let sets: Vec<LodId> = (self.bodies.values().copied())
            .chain(self.wheels.values().copied())
            .chain(self.leaves.values().copied())
            .chain(self.shells.values().copied())
            .chain(doors)
            .collect();
        for id in sets {
            let _ = world.set_lod_places(id, &[]);
        }
        self.near.clear();
        self.riding.clear();
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

    /// The moving vehicles near the camera this frame, nearest first
    /// (wrecks are not among them).
    pub fn riding(&self) -> &[Vehicle] {
        &self.riding
    }

    /// Wreck `id` as it stands (knocked round and leaning).
    pub fn wreck_of(&self, id: u64) -> Option<Vehicle> {
        self.wrecks.get(&id).map(|w| w.0)
    }

    /// How many vehicles are on screen.
    pub fn shown(&self) -> usize {
        self.near.len()
    }

    /// Works out where the moving vehicles near `eye` stand at `time`:
    /// no Engine, so it runs on a worker while the main thread does other
    /// things. [`Self::apply`] hands the result to Engine.
    pub fn plan(&mut self, traffic: &Traffic, eye: DVec2, time: f64, dt: f64) -> TrafficPlan {
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
        let mut wheels: BTreeMap<WheelOf, Vec<Place>> = BTreeMap::new();
        let mut leaves: BTreeMap<(u8, usize), Vec<Place>> = BTreeMap::new();
        let mut riding = Vec::new();
        for &id in &self.near {
            if self.wrecks.contains_key(&id) {
                continue;
            }
            let Some(v) = traffic.vehicle(id, time) else {
                continue;
            };
            bodies.entry(v.kind).or_default().push(body_place(&v));
            let far = DVec2::new(v.pos.x, v.pos.z).distance(eye);
            match v.kind {
                VehicleKind::Car(model, _) if far < WHEEL_RADIUS => {
                    wheel_places(
                        &v,
                        WheelOf::Car(model),
                        wheels.entry(WheelOf::Car(model)).or_default(),
                    );
                }
                VehicleKind::Bus(livery) => {
                    if far < WHEEL_RADIUS {
                        wheel_places(&v, WheelOf::Bus, wheels.entry(WheelOf::Bus).or_default());
                    }
                    if far < LEAF_RADIUS {
                        let open = traffic
                            .bus_at_stop(id)
                            .map_or(0.0, |a| doors_open(a.standing));
                        leaf_places(&v, livery, open, &mut leaves);
                    }
                }
                _ => {}
            }
            if DVec2::new(v.pos.x, v.pos.z).distance(eye) < crate::riderview::RIDER_RADIUS {
                riding.push(v);
            }
        }
        TrafficPlan {
            eye,
            bodies,
            wheels,
            leaves,
            riding,
        }
    }

    /// Puts the vehicles of `plan` (and the wrecks) into the world.
    pub fn apply(&mut self, world: &mut World, plan: TrafficPlan) -> EngineResult<()> {
        self.models.collect();
        let TrafficPlan {
            eye,
            mut bodies,
            mut wheels,
            mut leaves,
            riding,
        } = plan;
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
            if let VehicleKind::Bus(livery) = v.kind {
                // A wrecked bus: its doors sprung half open.
                wheel_places(v, WheelOf::Bus, wheels.entry(WheelOf::Bus).or_default());
                let mut sprung = BTreeMap::new();
                leaf_places(v, livery, 0.3 + (id % 5) as f64 * 0.1, &mut sprung);
                for (k, list) in sprung {
                    leaves
                        .entry(k)
                        .or_default()
                        .extend(list.into_iter().map(crumpled));
                }
            }
            let VehicleKind::Car(model, paint) = v.kind else {
                bodies.entry(v.kind).or_default().push(body);
                continue;
            };
            // The doors sprung by the crash: hanging part open, each wreck
            // its own way (as a whole car until they are made).
            if let std::collections::btree_map::Entry::Vacant(e) = self.doors.entry(v.kind) {
                let Some(made) = self.models.doors.remove(&v.kind) else {
                    let _ = paint;
                    bodies.entry(v.kind).or_default().push(body);
                    continue;
                };
                let mut sets = Vec::new();
                for (levels, hinge, axis, open) in made {
                    sets.push((world.spawn_lod(levels)?, hinge, axis, open));
                }
                e.insert(sets);
            }
            wheel_places(
                v,
                WheelOf::Car(model),
                wheels.entry(WheelOf::Car(model)).or_default(),
            );
            shells.entry(v.kind).or_default().push(body);
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
        self.riding = riding;
        // Every set gets its places, an empty list when none is near.
        for kind in bodies.keys() {
            spawn_made(world, &mut self.bodies, &mut self.models.bodies, *kind)?;
        }
        for model in wheels.keys() {
            spawn_made(world, &mut self.wheels, &mut self.models.wheels, *model)?;
        }
        for (kind, id) in &self.bodies {
            let places = bodies.get(kind).map_or(&[][..], |v| &v[..]);
            world.set_lod_places(*id, places)?;
        }
        for (model, id) in &self.wheels {
            let places = wheels.get(model).map_or(&[][..], |v| &v[..]);
            world.set_lod_places(*id, places)?;
        }
        for key in leaves.keys() {
            spawn_made(world, &mut self.leaves, &mut self.models.leaves, *key)?;
        }
        for (key, id) in &self.leaves {
            let places = leaves.get(key).map_or(&[][..], |v| &v[..]);
            world.set_lod_places(*id, places)?;
        }
        for kind in shells.keys() {
            spawn_made(world, &mut self.shells, &mut self.models.shells, *kind)?;
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

/// Where the moving vehicles stand this frame (see [`TrafficView::plan`]).
pub struct TrafficPlan {
    eye: DVec2,
    bodies: BTreeMap<VehicleKind, Vec<Place>>,
    wheels: BTreeMap<WheelOf, Vec<Place>>,
    leaves: BTreeMap<(u8, usize), Vec<Place>>,
    /// The vehicles near enough to show who sits in them (`riderview`).
    riding: Vec<Vehicle>,
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
pub fn angles(v: &Vehicle) -> (f64, f64, f64) {
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

fn wheel_places(v: &Vehicle, of: WheelOf, out: &mut Vec<Place>) {
    let layout = match of {
        WheelOf::Car(model) => crate::propview::wheel_layout(model),
        WheelOf::Bus => Some(crate::carmesh::BUS_WHEELS),
    };
    let Some((x, zs, r)) = layout else {
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

/// A bus's door leaves, its doors `open` (0 to 1).
fn leaf_places(v: &Vehicle, livery: u8, open: f64, out: &mut BTreeMap<(u8, usize), Vec<Place>>) {
    let (yaw, pitch, roll) = angles(v);
    let q =
        DQuat::from_rotation_y(yaw) * DQuat::from_rotation_x(pitch) * DQuat::from_rotation_z(roll);
    for (door, at) in crate::carmesh::bus_door_leaves(open) {
        let p = v.pos + q * at;
        out.entry((livery, door)).or_default().push(
            Place::new(p.x as f32, p.y as f32, p.z as f32)
                .with_yaw_deg(yaw.to_degrees() as f32)
                .with_pitch_deg(pitch.to_degrees() as f32)
                .with_roll_deg(roll.to_degrees() as f32),
        );
    }
}
