//! Traffic: golden-age cars on the streets, hover cars and flying taxis in
//! lanes over the main roads.
//!
//! Like the citizens, every vehicle is a pure function of the town seed, its
//! id and the time, so every machine in a co-op game sees the same traffic
//! without it being sent. The lanes come from `hw_gen::urban::lanes`: closed
//! loops through the town. Each loop carries a number of vehicles spaced
//! evenly and driving at the loop's own speed, so they never catch up with
//! each other. Streets near the town's core and main roads are busier.
//!
//! The sky lanes follow the main roads at set heights above them (three
//! layers), so fliers keep clear of the buildings; most fliers are taxis.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_gen::urban::lanes::{plan_lanes, LaneLoop};
use hw_gen::urban::props::{CarModel, CAR_PAINTS};
use hw_gen::urban::world::TownWorld;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Heights of the sky lanes above the road under them (m).
pub const SKY_LAYERS: [f64; 3] = [34.0, 46.0, 58.0];
/// Grid cell for finding lanes near a point (m).
const CELL: f64 = 64.0;
/// Closest spacing of cars on one lane at the town's core, and in the
/// outskirts (m); fliers keep further apart.
const GAP_CORE: f64 = 26.0;
const GAP_EDGE: f64 = 110.0;
const SKY_GAP_CORE: f64 = 48.0;
const SKY_GAP_EDGE: f64 = 170.0;
/// Bit set in the ids of fliers.
pub const SKY_BIT: u64 = 1 << 40;

/// What a vehicle is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum VehicleKind {
    /// A car on the street (model, paint).
    Car(CarModel, u8),
    /// A private hover car in a sky lane (paint).
    SkyCar(u8),
    /// A flying taxi.
    Taxi,
}

/// A vehicle as seen at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vehicle {
    pub id: u64,
    pub kind: VehicleKind,
    /// Centre of its underside, in the town's frame.
    pub pos: DVec3,
    /// The way its nose points (unit; climbs and dips on hills).
    pub dir: DVec3,
    /// Lean into turns (radians, positive to the right), for fliers.
    pub roll: f64,
    /// Speed (m/s).
    pub speed: f64,
    /// Distance driven along its lane (turns the wheels).
    pub odometer: f64,
}

impl Vehicle {
    pub fn flies(&self) -> bool {
        self.id & SKY_BIT != 0
    }

    /// Yaw (radians, 0 = +z) and pitch (radians, positive nose up).
    pub fn yaw_pitch(&self) -> (f64, f64) {
        let flat = DVec2::new(self.dir.x, self.dir.z);
        (
            hw_core::det::atan2(flat.x, flat.y),
            hw_core::det::atan2(self.dir.y, flat.length()),
        )
    }
}

/// One loop of lanes and the vehicles on it.
struct Lane {
    path: LaneLoop,
    /// Vehicles on the loop.
    count: u32,
    speed: f64,
    /// Where vehicle 0 is at time 0.
    phase: f64,
    /// Height above the road (0 on the ground).
    lift: f64,
}

/// A stretch of lane in a grid cell: (sky, lane, from, to).
type Span = (bool, u32, f64, f64);

/// The town's traffic.
pub struct Traffic {
    seed: Seed,
    ground: Vec<Lane>,
    sky: Vec<Lane>,
    /// Stretches of lane by grid cell: (sky, lane, from, to).
    grid: BTreeMap<(i64, i64), Vec<Span>>,
    world: Arc<TownWorld>,
    /// Street lanes held up by a wreck: the time they stopped.
    stops: BTreeMap<u32, f64>,
}

impl Traffic {
    /// Plans the lanes and how many vehicles each carries (a fraction of a
    /// second for a big town).
    pub fn new(world: Arc<TownWorld>) -> Self {
        let seed = world.seed.child("traffic");
        let core = world.spawn_line().0;
        let reach = world.half().max(200.0) * 0.6;
        // Busy at the core, quiet at the edge (smoothly).
        let busy = |p: DVec2| {
            let t = (p.distance(core) / reach).clamp(0.0, 1.0);
            1.0 - t * t * (3.0 - 2.0 * t)
        };
        // Cars keep to the right half of the carriageway; country lanes and
        // the narrowest streets have no traffic.
        let streets: Vec<usize> = (0..world.roads.len())
            .filter(|&i| world.roads[i].half >= 2.5 && world.roads[i].pts.len() >= 2)
            .collect();
        let ground_loops = plan_lanes(&world, &streets, |r| r.half * 0.5, seed.child("ground"));
        let mains: Vec<usize> = (0..world.roads.len())
            .filter(|&i| world.roads[i].main)
            .collect();
        let sky_loops = plan_lanes(&world, &mains, |r| r.half * 0.4, seed.child("sky"));
        let make = |path: LaneLoop, sky: bool, i: usize| -> Lane {
            let s = seed
                .child(if sky { "sky lane" } else { "lane" })
                .child(i as u64);
            let mut rng = s.rng();
            // Average busyness along the loop.
            let len = path.len();
            let n = (len / 25.0).ceil().max(1.0) as usize;
            let b = (0..n)
                .map(|k| busy(path.sample(len * k as f64 / n as f64).0))
                .sum::<f64>()
                / n as f64;
            let b = (b * (0.7 + 0.6 * path.main)).min(1.0);
            let (near, far) = if sky {
                (SKY_GAP_CORE, SKY_GAP_EDGE)
            } else {
                (GAP_CORE, GAP_EDGE)
            };
            let gap = far + (near - far) * b;
            let want = len / gap * rng.range(0.7, 1.3);
            let count = want.floor() as u32 + u32::from(rng.unit() < want.fract());
            let (speed, lift) = if sky {
                (
                    rng.range(17.0, 25.0),
                    SKY_LAYERS[rng.index(SKY_LAYERS.len())],
                )
            } else {
                (rng.range(8.0, 12.5) * (1.0 + 0.25 * path.main), 0.0)
            };
            Lane {
                count: count.min(0xFFFF),
                speed,
                phase: rng.range(0.0, len),
                lift,
                path,
            }
        };
        let ground: Vec<Lane> = ground_loops
            .into_iter()
            .enumerate()
            .map(|(i, l)| make(l, false, i))
            .collect();
        let sky: Vec<Lane> = sky_loops
            .into_iter()
            .enumerate()
            .map(|(i, l)| make(l, true, i))
            .collect();
        let mut grid: BTreeMap<(i64, i64), Vec<Span>> = BTreeMap::new();
        for (flies, lanes) in [(false, &ground), (true, &sky)] {
            for (i, l) in lanes.iter().enumerate() {
                if l.count == 0 {
                    continue;
                }
                for (c, a, b) in l.path.spans(CELL) {
                    grid.entry(c).or_default().push((flies, i as u32, a, b));
                }
            }
        }
        Self {
            seed,
            ground,
            sky,
            grid,
            world,
            stops: BTreeMap::new(),
        }
    }

    /// Holds up the street lanes with wrecks on them: each stands still
    /// from the earliest wreck's time (see [`crate::wrecks`]).
    pub fn set_stops(&mut self, wrecks: impl IntoIterator<Item = (u64, f64)>) {
        self.stops.clear();
        for (id, time) in wrecks {
            if id & SKY_BIT != 0 {
                continue;
            }
            let lane = (id >> 16) as u32;
            let t = self.stops.entry(lane).or_insert(time);
            *t = t.min(time);
        }
    }

    /// The time lane `lane` has reached by `time` (stopped lanes stand).
    fn lane_time(&self, flies: bool, lane: u32, time: f64) -> f64 {
        match (flies, self.stops.get(&lane)) {
            (false, Some(&t)) => t.min(time),
            _ => time,
        }
    }

    /// Vehicles on the streets and in the air.
    pub fn count(&self) -> (u64, u64) {
        let n = |l: &[Lane]| l.iter().map(|x| x.count as u64).sum();
        (n(&self.ground), n(&self.sky))
    }

    /// Every vehicle within `radius` (on the ground plan) of `around` at
    /// `time`, nearest first, at most `max`.
    pub fn near(&self, around: DVec2, radius: f64, time: f64, max: usize) -> Vec<Vehicle> {
        let c0 = cell_of(around - DVec2::splat(radius));
        let c1 = cell_of(around + DVec2::splat(radius));
        let mut ids = Vec::new();
        for x in c0.0..=c1.0 {
            for (_, list) in self.grid.range((x, c0.1)..=(x, c1.1)) {
                for &(flies, lane, s0, s1) in list {
                    let l = self.lane(flies, lane);
                    let len = l.path.len();
                    let gap = len / l.count as f64;
                    let t = self.lane_time(flies, lane, time);
                    let base = (l.phase + l.speed * t).rem_euclid(len);
                    // Vehicle k sits at base + k * gap (round the loop).
                    let u0 = (s0 - base).rem_euclid(len);
                    let first = (u0 / gap).ceil() as i64;
                    let last = ((u0 + (s1 - s0)) / gap).floor() as i64;
                    for k in first..=last {
                        let k = k.rem_euclid(l.count as i64) as u64;
                        ids.push(id_of(flies, lane, k));
                    }
                }
            }
        }
        ids.sort();
        ids.dedup();
        let mut out: Vec<Vehicle> = ids
            .into_iter()
            .filter_map(|id| self.vehicle(id, time))
            .filter(|v| DVec2::new(v.pos.x, v.pos.z).distance(around) <= radius)
            .collect();
        out.sort_by(|a, b| {
            let d = |v: &Vehicle| DVec2::new(v.pos.x, v.pos.z).distance_squared(around);
            d(a).total_cmp(&d(b)).then(a.id.cmp(&b.id))
        });
        out.truncate(max);
        out
    }

    fn lane(&self, flies: bool, i: u32) -> &Lane {
        if flies {
            &self.sky[i as usize]
        } else {
            &self.ground[i as usize]
        }
    }

    /// Vehicle `id` at `time`; `None` if there is no such vehicle.
    pub fn vehicle(&self, id: u64, time: f64) -> Option<Vehicle> {
        let flies = id & SKY_BIT != 0;
        let lane = ((id & !SKY_BIT) >> 16) as u32;
        let k = id & 0xFFFF;
        let lanes = if flies { &self.sky } else { &self.ground };
        let l = lanes.get(lane as usize)?;
        if k >= l.count as u64 {
            return None;
        }
        let len = l.path.len();
        let stopped = self.lane_time(flies, lane, time) < time;
        let lt = self.lane_time(flies, lane, time);
        let s = l.phase + l.speed * lt + len * k as f64 / l.count as f64;
        let (p, d) = l.path.sample(s);
        let mut rng = self.seed.child(id).rng();
        let kind = if flies {
            if rng.chance(0.65) {
                VehicleKind::Taxi
            } else {
                VehicleKind::SkyCar(rng.range_int(0, CAR_PAINTS as i64) as u8)
            }
        } else {
            let model = [CarModel::Cruiser, CarModel::Bubble, CarModel::Hover]
                [rng.weighted(&[0.45, 0.35, 0.2])];
            VehicleKind::Car(model, rng.range_int(0, CAR_PAINTS as i64) as u8)
        };
        let road = |q: DVec2| self.world.road_height(q);
        let (pos, dir, roll) = if flies {
            // Height follows the ground smoothly, banking into the turns.
            let h = |s: f64| {
                let mut sum = 0.0;
                for j in -2..=2 {
                    sum += road(l.path.sample(s + j as f64 * 15.0).0);
                }
                sum / 5.0 + l.lift
            };
            let (pa, da) = l.path.sample(s - 8.0);
            let (pb, db) = l.path.sample(s + 8.0);
            let turn = hw_core::det::atan2(da.perp_dot(db), da.dot(db));
            let roll = (turn * l.speed * 0.06).clamp(-0.45, 0.45);
            let climb = (h(s + 8.0) - h(s - 8.0)) / pa.distance(pb).max(1.0);
            let dir = DVec3::new(d.x, climb, d.y).normalize();
            let bob = hw_core::det::sin(time * 1.3 + (id % 97) as f64) * 0.25;
            (DVec3::new(p.x, h(s) + bob, p.y), dir, roll)
        } else {
            let (pf, _) = l.path.sample(s + 1.6);
            let (pb, _) = l.path.sample(s - 1.6);
            let (yf, yb) = (road(pf), road(pb));
            let lift = if matches!(kind, VehicleKind::Car(CarModel::Hover, _)) {
                0.25
            } else {
                0.0
            };
            let dir = DVec3::new(pf.x - pb.x, yf - yb, pf.y - pb.y).normalize_or(DVec3::Z);
            (DVec3::new(p.x, (yf + yb) * 0.5 + lift, p.y), dir, 0.0)
        };
        Some(Vehicle {
            id,
            kind,
            pos,
            dir,
            roll,
            speed: if stopped { 0.0 } else { l.speed },
            odometer: s,
        })
    }
}

fn id_of(flies: bool, lane: u32, k: u64) -> u64 {
    (if flies { SKY_BIT } else { 0 }) | ((lane as u64) << 16) | k
}

fn cell_of(p: DVec2) -> (i64, i64) {
    ((p.x / CELL).floor() as i64, (p.y / CELL).floor() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GeneratedTown;

    fn traffic() -> Traffic {
        let t = GeneratedTown::generate(Seed(1), 20_000.0);
        Traffic::new(t.world)
    }

    #[test]
    fn streets_and_sky_are_busy_and_vehicles_move_smoothly() {
        let t = traffic();
        let (cars, fliers) = t.count();
        assert!(cars > 200, "{cars} cars");
        assert!(fliers > 20, "{fliers} fliers");
        let p = t.world.spawn_line().0;
        let near = t.near(p, 400.0, 100.0, 1000);
        assert!(near.len() > 20, "{} near the core", near.len());
        assert!(near.iter().any(|v| v.flies()));
        for a in &near {
            let b = t.vehicle(a.id, 100.0 + 1.0 / 30.0).unwrap();
            assert!(a.pos.is_finite());
            // A tick's drive, no more (a little extra round tight corners).
            let step = a.pos.distance(b.pos);
            assert!(step < a.speed / 30.0 + 0.5, "{} jumped {step} m", a.id);
            if !a.flies() {
                let ground = t.world.road_height(DVec2::new(a.pos.x, a.pos.z));
                assert!((a.pos.y - ground).abs() < 1.5, "car off the road");
            }
        }
    }

    #[test]
    fn cars_on_one_lane_keep_their_distance() {
        let t = traffic();
        for l in t.ground.iter().filter(|l| l.count > 0) {
            let gap = l.path.len() / l.count as f64;
            assert!(gap > GAP_CORE * 0.6, "cars {gap:.1} m apart");
        }
        for l in t.sky.iter().filter(|l| l.count > 0) {
            assert!(l.path.len() / l.count as f64 > SKY_GAP_CORE * 0.6);
        }
    }

    #[test]
    fn same_seed_same_traffic() {
        let (a, b) = (traffic(), traffic());
        let p = a.world.spawn_line().0;
        assert_eq!(a.near(p, 300.0, 7.0, 400), b.near(p, 300.0, 7.0, 400));
    }
}
