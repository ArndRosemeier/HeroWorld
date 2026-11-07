//! The game server. It owns the truth: every entity, every hit point, every
//! broken wall. Clients (including the local one in single player) only show
//! and predict what this crate decides.
//!
//! The state lives in a `bevy_ecs` world and advances in fixed ticks
//! ([`TICK_HZ`]). [`host::Host`] puts a network socket around it.

pub mod host;

use bevy_ecs::prelude::*;
use glam::DVec3;
use hw_core::{NetId, NetIdAllocator, Seed};
use hw_net::{EntityState, InputState, PlayerId};

/// Simulation ticks per second.
pub const TICK_HZ: u32 = 30;
/// Seconds per tick.
pub const DT: f64 = 1.0 / TICK_HZ as f64;

/// Ground height in the city's local frame until terrain exists (top of
/// streets and sidewalks).
pub const GROUND_Y: f64 = 0.26;
/// A normal person's top running speed (m/s) and acceleration.
pub const RUN_SPEED: f64 = 7.0;
pub const RUN_ACCEL: f64 = 40.0;
pub const GRAVITY: f64 = 20.0;
pub const JUMP_SPEED: f64 = 7.5;

/// Every replicated entity has one.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Net(pub NetId);

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Position(pub DVec3);

#[derive(Component, Clone, Copy, Debug, PartialEq, Default)]
pub struct Velocity(pub DVec3);

/// A player's hero.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hero {
    pub owner: PlayerId,
}

/// The newest input from the hero's player.
#[derive(Component, Clone, Copy, Debug, PartialEq, Default)]
pub struct Control(pub InputState);

#[derive(Component, Clone, Copy, Debug, PartialEq, Default)]
pub struct Grounded(pub bool);

#[derive(Resource)]
struct Ids(NetIdAllocator);

/// The server's simulation state.
pub struct Sim {
    pub world: World,
    pub seed: Seed,
    pub tick: u64,
    schedule: Schedule,
}

impl Sim {
    pub fn new(seed: Seed) -> Self {
        let mut world = World::new();
        world.insert_resource(Ids(NetIdAllocator::new()));
        let mut schedule = Schedule::default();
        schedule.add_systems((control_heroes, integrate).chain());
        Self {
            world,
            seed,
            tick: 0,
            schedule,
        }
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
            Position(at),
            Velocity::default(),
            Hero { owner },
            Control::default(),
            Grounded(false),
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

    pub fn position(&mut self, id: NetId) -> Option<DVec3> {
        self.world
            .query::<(&Net, &Position)>()
            .iter(&self.world)
            .find(|(n, _)| n.0 == id)
            .map(|(_, p)| p.0)
    }

    /// The state of every replicated entity, sorted by ID.
    pub fn snapshot(&mut self) -> Vec<EntityState> {
        let mut out: Vec<EntityState> = self
            .world
            .query::<(&Net, &Position, &Velocity, Option<&Hero>)>()
            .iter(&self.world)
            .map(|(n, p, v, h)| EntityState {
                id: n.0,
                pos: p.0.to_array(),
                vel: v.0.as_vec3().to_array(),
                owner: h.map(|h| h.owner),
            })
            .collect();
        out.sort_by_key(|e| e.id);
        out
    }
}

/// Turns input into velocity: run towards the wanted direction, jump when on
/// the ground.
fn control_heroes(mut q: Query<(&Control, &mut Velocity, &Grounded)>) {
    for (control, mut vel, grounded) in &mut q {
        let input = control.0;
        let want = DVec3::new(input.move_x as f64, 0.0, input.move_z as f64) * RUN_SPEED;
        let horizontal = DVec3::new(vel.0.x, 0.0, vel.0.z);
        let delta = want - horizontal;
        let max = RUN_ACCEL * DT;
        let step = if delta.length() > max {
            delta.normalize() * max
        } else {
            delta
        };
        vel.0 += step;
        if input.jump && grounded.0 {
            vel.0.y = JUMP_SPEED;
        }
    }
}

/// Gravity, motion, and the ground.
fn integrate(mut q: Query<(&mut Position, &mut Velocity, &mut Grounded)>) {
    for (mut pos, mut vel, mut grounded) in &mut q {
        vel.0.y -= GRAVITY * DT;
        pos.0 += vel.0 * DT;
        if pos.0.y <= GROUND_Y {
            pos.0.y = GROUND_Y;
            vel.0.y = vel.0.y.max(0.0);
            grounded.0 = true;
        } else {
            grounded.0 = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_count_ticks() {
        let mut sim = Sim::new(Seed(1));
        for _ in 0..3 {
            sim.step();
        }
        assert_eq!(sim.tick, 3);
    }

    #[test]
    fn a_hero_runs_where_told_and_lands() {
        let mut sim = Sim::new(Seed(1));
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
        let mut sim = Sim::new(Seed(1));
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
            let mut sim = Sim::new(Seed(4));
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
    fn removing_a_hero() {
        let mut sim = Sim::new(Seed(1));
        sim.spawn_hero(PlayerId(1), DVec3::ZERO);
        sim.spawn_hero(PlayerId(2), DVec3::ZERO);
        sim.remove_hero(PlayerId(1));
        let snap = sim.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].owner, Some(PlayerId(2)));
    }
}
