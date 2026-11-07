//! The game server. It owns the truth: every entity, every hit point, every
//! broken wall. Clients (including the local one in single player) only show
//! and predict what this crate decides.
//!
//! The state lives in a `bevy_ecs` world and advances in fixed ticks
//! ([`TICK_HZ`]). [`host::Host`] puts a network socket around it.

pub mod collision;
pub mod host;
pub mod predict;

use bevy_ecs::prelude::*;
use collision::{Body, Obstacles};
use glam::{DVec2, DVec3};
use hw_core::{NetId, NetIdAllocator, Seed};
use hw_gen::building::BuildingPlan;
use hw_gen::city::{CityParams, CityPlan};
use hw_net::{EntityState, InputState, PlayerId};
use std::sync::Arc;

/// Simulation ticks per second.
pub const TICK_HZ: u32 = 30;
/// Seconds per tick.
pub const DT: f64 = 1.0 / TICK_HZ as f64;

/// Height of open ground in the city's local frame until terrain exists (top
/// of streets and sidewalks).
pub const GROUND_Y: f64 = 0.26;
/// Where buildings stand (top of the block ground).
pub const BUILDING_BASE: f64 = 0.22;
/// A normal person's top running speed (m/s) and acceleration.
pub const RUN_SPEED: f64 = 7.0;
pub const RUN_ACCEL: f64 = 40.0;
/// Steering in the air is weaker.
pub const AIR_ACCEL: f64 = 12.0;
pub const GRAVITY: f64 = 20.0;
pub const JUMP_SPEED: f64 = 7.5;

/// Every replicated entity has one.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Net(pub NetId);

/// Where a moving entity is, how fast it goes, whether it stands on something.
#[derive(Component, Clone, Copy, Debug, PartialEq, Default)]
pub struct Motion(pub Body);

/// A player's hero.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hero {
    pub owner: PlayerId,
}

/// The newest input from the hero's player.
#[derive(Component, Clone, Copy, Debug, PartialEq, Default)]
pub struct Control(pub InputState);

#[derive(Resource)]
struct Ids(NetIdAllocator);

/// The solid city, shared with anything else that needs it.
#[derive(Resource, Clone)]
pub struct Solids(pub Arc<Obstacles>);

/// The city every machine generates from the seed, with its building plans
/// and collision.
pub struct GeneratedCity {
    pub plan: CityPlan,
    pub buildings: Vec<BuildingPlan>,
    pub obstacles: Arc<Obstacles>,
}

impl GeneratedCity {
    pub fn generate(seed: Seed) -> Self {
        let plan = CityPlan::generate(seed, &CityParams::default());
        let buildings: Vec<BuildingPlan> = plan
            .buildings
            .iter()
            .map(|(_, s)| BuildingPlan::generate(s))
            .collect();
        let obstacles = Arc::new(Obstacles::from_city(
            &plan,
            &buildings,
            GROUND_Y,
            BUILDING_BASE,
        ));
        Self {
            plan,
            buildings,
            obstacles,
        }
    }

    /// Spawn points on the street closest to the civic centre.
    pub fn spawn_line(&self) -> (DVec2, DVec2) {
        let street = self.plan.streets.iter().min_by(|a, b| {
            a.distance_to(DVec2::ZERO)
                .total_cmp(&b.distance_to(DVec2::ZERO))
        });
        match street {
            Some(s) => ((s.a + s.b) * 0.5, (s.b - s.a).normalize_or(DVec2::X)),
            None => (DVec2::ZERO, DVec2::X),
        }
    }
}

/// One tick of a hero's movement. The server and client prediction both run
/// exactly this, so they agree.
pub fn step_hero(body: &mut Body, input: InputState, solids: &Obstacles) {
    let want = DVec3::new(input.move_x as f64, 0.0, input.move_z as f64) * RUN_SPEED;
    let horizontal = DVec3::new(body.vel.x, 0.0, body.vel.z);
    let delta = want - horizontal;
    let accel = if body.grounded { RUN_ACCEL } else { AIR_ACCEL };
    let max = accel * DT;
    body.vel += if delta.length() > max {
        delta.normalize() * max
    } else {
        delta
    };
    if input.jump && body.grounded {
        body.vel.y = JUMP_SPEED;
    }
    body.vel.y -= GRAVITY * DT;
    solids.move_body(body, DT);
}

/// The server's simulation state.
pub struct Sim {
    pub world: World,
    pub seed: Seed,
    pub tick: u64,
    spawn: (DVec2, DVec2),
    schedule: Schedule,
}

impl Sim {
    /// A simulation of the city generated from `seed`.
    pub fn new(seed: Seed) -> Self {
        Self::for_city(seed, &GeneratedCity::generate(seed))
    }

    /// A simulation of a city already generated (the client shares its copy).
    pub fn for_city(seed: Seed, city: &GeneratedCity) -> Self {
        let mut sim = Self::with_solids(seed, city.obstacles.clone());
        sim.spawn = city.spawn_line();
        sim
    }

    /// A simulation with the given solids (tests use flat ground).
    pub fn with_solids(seed: Seed, solids: Arc<Obstacles>) -> Self {
        let mut world = World::new();
        world.insert_resource(Ids(NetIdAllocator::new()));
        world.insert_resource(Solids(solids));
        let mut schedule = Schedule::default();
        schedule.add_systems(move_heroes);
        Self {
            world,
            seed,
            tick: 0,
            spawn: (DVec2::ZERO, DVec2::X),
            schedule,
        }
    }

    pub fn solids(&self) -> Arc<Obstacles> {
        self.world.resource::<Solids>().0.clone()
    }

    /// Where the `k`-th player appears: in a row along the central street.
    pub fn spawn_point(&self, k: u32) -> DVec3 {
        let (mid, dir) = self.spawn;
        let offset = (k as f64 - 1.0) * 2.5 * if k.is_multiple_of(2) { 1.0 } else { -1.0 };
        let p = mid + dir * offset;
        DVec3::new(p.x, GROUND_Y + 0.5, p.y)
    }

    /// Advance the simulation by one fixed tick.
    pub fn step(&mut self) {
        self.schedule.run(&mut self.world);
        self.tick += 1;
    }

    /// Spawns a hero for `owner` at `at` and returns its ID.
    pub fn spawn_hero(&mut self, owner: PlayerId, at: DVec3) -> NetId {
        let id = self.world.resource_mut::<Ids>().0.allocate();
        self.world.spawn((
            Net(id),
            Motion(Body {
                pos: at,
                vel: DVec3::ZERO,
                grounded: false,
            }),
            Hero { owner },
            Control::default(),
        ));
        id
    }

    pub fn remove_hero(&mut self, owner: PlayerId) {
        let found: Vec<Entity> = self
            .world
            .query::<(Entity, &Hero)>()
            .iter(&self.world)
            .filter(|(_, h)| h.owner == owner)
            .map(|(e, _)| e)
            .collect();
        for e in found {
            self.world.despawn(e);
        }
    }

    pub fn set_input(&mut self, owner: PlayerId, input: InputState) {
        for (hero, mut control) in self
            .world
            .query::<(&Hero, &mut Control)>()
            .iter_mut(&mut self.world)
        {
            if hero.owner == owner {
                control.0 = input;
            }
        }
    }

    pub fn hero_of(&mut self, owner: PlayerId) -> Option<NetId> {
        self.world
            .query::<(&Net, &Hero)>()
            .iter(&self.world)
            .find(|(_, h)| h.owner == owner)
            .map(|(n, _)| n.0)
    }

    pub fn body(&mut self, id: NetId) -> Option<Body> {
        self.world
            .query::<(&Net, &Motion)>()
            .iter(&self.world)
            .find(|(n, _)| n.0 == id)
            .map(|(_, m)| m.0)
    }

    pub fn position(&mut self, id: NetId) -> Option<DVec3> {
        self.body(id).map(|b| b.pos)
    }

    /// The state of every replicated entity, sorted by ID.
    pub fn snapshot(&mut self) -> Vec<EntityState> {
        let mut out: Vec<EntityState> = self
            .world
            .query::<(&Net, &Motion, Option<&Hero>)>()
            .iter(&self.world)
            .map(|(n, m, h)| EntityState {
                id: n.0,
                pos: m.0.pos.to_array(),
                vel: m.0.vel.as_vec3().to_array(),
                grounded: m.0.grounded,
                owner: h.map(|h| h.owner),
            })
            .collect();
        out.sort_by_key(|e| e.id);
        out
    }
}

fn move_heroes(solids: Res<Solids>, mut q: Query<(&Control, &mut Motion)>) {
    for (control, mut motion) in &mut q {
        step_hero(&mut motion.0, control.0, &solids.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_sim(seed: u64) -> Sim {
        Sim::with_solids(Seed(seed), Arc::new(Obstacles::flat(GROUND_Y)))
    }

    #[test]
    fn steps_count_ticks() {
        let mut sim = flat_sim(1);
        for _ in 0..3 {
            sim.step();
        }
        assert_eq!(sim.tick, 3);
    }

    #[test]
    fn a_hero_runs_where_told_and_lands() {
        let mut sim = flat_sim(1);
        let p = PlayerId(1);
        let id = sim.spawn_hero(p, DVec3::new(0.0, 5.0, 0.0));
        sim.set_input(
            p,
            InputState {
                move_x: 1.0,
                move_z: 0.0,
                jump: false,
            },
        );
        for _ in 0..TICK_HZ * 2 {
            sim.step();
        }
        let pos = sim.position(id).unwrap();
        assert_eq!(pos.y, GROUND_Y);
        // Two seconds at up to 7 m/s, after a short run-up.
        assert!(pos.x > 11.0 && pos.x < 14.0, "x = {}", pos.x);
        assert!(pos.z.abs() < 1e-9);
    }

    #[test]
    fn jumping_needs_ground() {
        let mut sim = flat_sim(1);
        let p = PlayerId(1);
        let id = sim.spawn_hero(p, DVec3::new(0.0, GROUND_Y, 0.0));
        sim.step();
        sim.set_input(
            p,
            InputState {
                jump: true,
                ..Default::default()
            },
        );
        sim.step();
        let up = sim.position(id).unwrap().y;
        assert!(up > GROUND_Y);
        // Holding jump in the air does not climb further than one jump allows.
        for _ in 0..TICK_HZ * 3 {
            sim.step();
        }
        let peak = JUMP_SPEED * JUMP_SPEED / (2.0 * GRAVITY);
        assert!(sim.position(id).unwrap().y <= GROUND_Y + peak + 0.5);
    }

    #[test]
    fn same_inputs_same_result() {
        let run = || {
            let mut sim = flat_sim(4);
            for k in 1..=3u16 {
                sim.spawn_hero(PlayerId(k), DVec3::new(k as f64 * 3.0, 0.3, 0.0));
            }
            for t in 0..90 {
                for k in 1..=3u16 {
                    let a = (t * k as u32) as f32 * 0.1;
                    sim.set_input(
                        PlayerId(k),
                        InputState {
                            move_x: a.cos(),
                            move_z: a.sin(),
                            jump: t % 20 == 0,
                        },
                    );
                }
                sim.step();
            }
            sim.snapshot()
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn heroes_spawn_on_open_street_in_the_city() {
        let mut sim = Sim::new(Seed(3));
        for k in 1..=10u16 {
            let id = sim.spawn_hero(PlayerId(k), sim.spawn_point(k as u32));
            let _ = id;
        }
        for _ in 0..TICK_HZ {
            sim.step();
        }
        for e in sim.snapshot() {
            assert!(e.grounded, "hero {:?} not standing", e.id);
            assert!(
                (e.pos[1] - GROUND_Y).abs() < 1e-9,
                "hero {:?} at height {}",
                e.id,
                e.pos[1]
            );
        }
    }

    #[test]
    fn removing_a_hero() {
        let mut sim = flat_sim(1);
        sim.spawn_hero(PlayerId(1), DVec3::ZERO);
        sim.spawn_hero(PlayerId(2), DVec3::ZERO);
        sim.remove_hero(PlayerId(1));
        let snap = sim.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].owner, Some(PlayerId(2)));
    }
}
