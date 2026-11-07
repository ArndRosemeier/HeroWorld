//! The game server. It owns the truth: every entity, every hit point, every
//! broken wall. Clients (including the local one in single player) only show
//! and predict what this crate decides.
//!
//! The state lives in a `bevy_ecs` world and advances in fixed ticks
//! ([`TICK_HZ`]). [`host::Host`] puts a network socket around it.

pub mod bubbles;
pub mod collision;
pub mod host;
pub mod physics;
pub mod predict;
pub mod save;

use bevy_ecs::prelude::*;
use bubbles::Bubbles;
use collision::{Body, Obstacles};
use glam::{DVec2, DVec3};
use hw_core::{NetId, NetIdAllocator, Seed};
use hw_gen::building::BuildingPlan;
use hw_gen::city::{CityParams, CityPlan};
use hw_gen::urban::world::TownWorld;
use hw_gen::urban::TownParams;
use hw_net::{EntityKind, EntityState, InputState, PlayerId};
use physics::{Physics, PropState, CRATE_HALF};
use std::collections::BTreeSet;
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
/// People in the town when nobody says otherwise.
pub const DEFAULT_POPULATION: f64 = 40_000.0;
/// A normal person's top running speed (m/s) and acceleration.
pub const RUN_SPEED: f64 = 7.0;
pub const RUN_ACCEL: f64 = 40.0;
/// Steering in the air is weaker.
pub const AIR_ACCEL: f64 = 12.0;
pub const GRAVITY: f64 = 20.0;
pub const JUMP_SPEED: f64 = 7.5;
/// Vertical speed in a lift shaft.
pub const LIFT_SPEED: f64 = 5.0;
/// Flight: cruising speed, with boost, and how quickly it is reached.
pub const FLY_SPEED: f64 = 22.0;
pub const FLY_BOOST_SPEED: f64 = 60.0;
pub const FLY_ACCEL: f64 = 35.0;
pub const FLY_BOOST_ACCEL: f64 = 70.0;
/// Climb or sink speed from jump/descend while flying (doubled with boost).
pub const FLY_CLIMB: f64 = 10.0;
/// Highest a flier can go for now (the sky above the city).
pub const FLY_CEILING: f64 = 900.0;

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

/// A loose physics body (a crate); its [`Motion`] position is its centre.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Prop;

/// Which way a prop faces: a quaternion (x, y, z, w).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Orientation(pub [f32; 4]);

/// The tick an entity last changed (moved, turned, appeared), from
/// `bevy_ecs` change detection. Clients get an entity again only when this
/// is newer than what they confirmed.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct LastChange(pub u64);

#[derive(Resource)]
struct Ids(NetIdAllocator);

#[derive(Resource)]
struct Now(u64);

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
    /// The default city (a big city) of `seed`.
    pub fn generate(seed: Seed) -> Self {
        Self::generate_with(seed, &CityParams::default())
    }

    /// The city of `seed` at the size `params` give.
    pub fn generate_with(seed: Seed, params: &CityParams) -> Self {
        let plan = CityPlan::generate(seed, params);
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
        let street = self
            .plan
            .streets
            .iter()
            .filter(|s| !s.class.is_water() && s.a.distance(s.b) > 10.0)
            .filter(|s| !self.plan.water.is_water((s.a + s.b) * 0.5))
            .min_by(|a, b| {
                a.distance_to(DVec2::ZERO)
                    .total_cmp(&b.distance_to(DVec2::ZERO))
            });
        match street {
            Some(s) => ((s.a + s.b) * 0.5, (s.b - s.a).normalize_or(DVec2::X)),
            None => (DVec2::ZERO, DVec2::X),
        }
    }
}

/// The town every machine generates from the seed and its size, with its
/// collision.
pub struct GeneratedTown {
    pub world: Arc<TownWorld>,
    pub obstacles: Arc<Obstacles>,
}

impl GeneratedTown {
    /// The town of `seed` with about `population` people.
    pub fn generate(seed: Seed, population: f64) -> Self {
        let world = Arc::new(TownWorld::generate(
            seed,
            TownParams::sample(seed, population),
        ));
        let w = world.clone();
        let terrain = collision::Terrain(Arc::new(move |p| w.ground(p)));
        let obstacles = Arc::new(Obstacles::from_town(&world, terrain));
        Self { world, obstacles }
    }

    pub fn population(&self) -> f64 {
        self.world.town.params.population
    }
}

/// One tick of a hero's movement. The server and client prediction both run
/// exactly this, so they agree.
pub fn step_hero(body: &mut Body, input: InputState, solids: &Obstacles) {
    if input.fly {
        fly_step(body, input);
        solids.move_body(body, DT);
        if body.pos.y > FLY_CEILING {
            body.pos.y = FLY_CEILING;
            body.vel.y = body.vel.y.min(0.0);
        }
        return;
    }
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
    if solids.in_lift(body.pos) {
        // The Aether lift in every core: hold jump to rise, descend to sink,
        // otherwise hover. Step out at any floor.
        body.vel.y = if input.jump {
            LIFT_SPEED
        } else if input.descend {
            -LIFT_SPEED
        } else {
            0.0
        };
    } else {
        if input.jump && body.grounded {
            body.vel.y = JUMP_SPEED;
        }
        body.vel.y -= GRAVITY * DT;
    }
    solids.move_body(body, DT);
}

/// Flight: no gravity; the velocity turns toward where the hero steers
/// (move in three dimensions, plus jump/descend to climb or sink) and comes
/// to a hover when nothing is held.
fn fly_step(body: &mut Body, input: InputState) {
    let (speed, accel, climb) = if input.boost {
        (FLY_BOOST_SPEED, FLY_BOOST_ACCEL, FLY_CLIMB * 2.0)
    } else {
        (FLY_SPEED, FLY_ACCEL, FLY_CLIMB)
    };
    let steer = DVec3::new(
        input.move_x as f64,
        input.move_y as f64,
        input.move_z as f64,
    );
    let mut want = steer * speed;
    if input.jump {
        want.y += climb;
    }
    if input.descend {
        want.y -= climb;
    }
    let delta = want - body.vel;
    // Braking is quicker than speeding up, so a flier stops where wanted.
    let a = if want.length_squared() < body.vel.length_squared() * 0.5 {
        accel * 1.5
    } else {
        accel
    };
    let max = a * DT;
    body.vel += if delta.length() > max {
        delta.normalize() * max
    } else {
        delta
    };
}

/// The server's simulation state.
pub struct Sim {
    pub world: World,
    pub seed: Seed,
    /// Size of the city: sent to joining players with the seed.
    pub town_population: f64,
    pub tick: u64,
    spawn: (DVec2, DVec2),
    schedule: Schedule,
    track: Schedule,
    /// Engine physics for loose bodies.
    pub physics: Physics,
    /// The simulated areas around the players.
    pub bubbles: Bubbles,
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

    /// A simulation of a town already generated (the client shares its copy).
    pub fn for_town(seed: Seed, town: &GeneratedTown) -> Self {
        let mut sim = Self::with_solids(seed, town.obstacles.clone());
        sim.spawn = town.world.spawn_line();
        sim.town_population = town.population();
        sim
    }

    /// A simulation with the given solids (tests use flat ground).
    pub fn with_solids(seed: Seed, solids: Arc<Obstacles>) -> Self {
        let mut world = World::new();
        world.insert_resource(Ids(NetIdAllocator::new()));
        world.insert_resource(Solids(solids.clone()));
        world.insert_resource(Now(0));
        let mut schedule = Schedule::default();
        schedule.add_systems(move_heroes);
        let mut track = Schedule::default();
        track.add_systems(track_changes);
        Self {
            world,
            seed,
            town_population: DEFAULT_POPULATION,
            tick: 0,
            spawn: (DVec2::ZERO, DVec2::X),
            schedule,
            track,
            physics: Physics::new(solids),
            bubbles: Bubbles::default(),
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
        let ground = self.world.resource::<Solids>().0.ground_at(p).max(GROUND_Y);
        DVec3::new(p.x, ground + 0.5, p.y)
    }

    /// Advance the simulation by one fixed tick: heroes move, bubbles
    /// regroup, physics steps inside the bubbles, changes are stamped.
    pub fn step(&mut self) {
        self.schedule.run(&mut self.world);
        let heroes: Vec<(PlayerId, NetId, DVec3)> = self
            .world
            .query::<(&Hero, &Net, &Motion)>()
            .iter(&self.world)
            .map(|(h, n, m)| (h.owner, n.0, m.0.pos))
            .collect();
        let players: Vec<(PlayerId, DVec3)> = heroes.iter().map(|(p, _, at)| (*p, *at)).collect();
        self.bubbles.update(&players);
        self.step_physics(&heroes);
        self.tick += 1;
        self.world.resource_mut::<Now>().0 = self.tick;
        self.track.run(&mut self.world);
    }

    fn step_physics(&mut self, heroes: &[(PlayerId, NetId, DVec3)]) {
        let keep: BTreeSet<NetId> = heroes.iter().map(|(_, id, _)| *id).collect();
        self.physics.keep_heroes(&keep);
        for (_, id, at) in heroes {
            self.physics.place_hero(*id, *at);
        }
        // Props inside a bubble are simulated, nearest the bubble's centre
        // first, up to its budget; the rest hold still where they are.
        let mut props: Vec<(Entity, NetId, Body, [f32; 4])> = self
            .world
            .query_filtered::<(Entity, &Net, &Motion, &Orientation), With<Prop>>()
            .iter(&self.world)
            .map(|(e, n, m, o)| (e, n.0, m.0, o.0))
            .collect();
        props.sort_by_key(|p| p.1);
        let mut ranked: Vec<(usize, f64, NetId)> = props
            .iter()
            .filter_map(|(_, id, b, _)| {
                let (i, bubble) = self
                    .bubbles
                    .list
                    .iter()
                    .enumerate()
                    .filter(|(_, x)| x.contains(b.pos))
                    .min_by(|x, y| {
                        x.1.center
                            .distance_squared(b.pos)
                            .total_cmp(&y.1.center.distance_squared(b.pos))
                    })?;
                Some((i, bubble.center.distance(b.pos), *id))
            })
            .collect();
        ranked.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));
        let mut used = vec![0usize; self.bubbles.len()];
        let mut awake = BTreeSet::new();
        for (i, _, id) in ranked {
            if used[i] < self.bubbles.list[i].budget {
                used[i] += 1;
                awake.insert(id);
            }
        }
        for (_, id, body, rot) in &props {
            match (awake.contains(id), self.physics.is_awake(*id)) {
                (true, false) => self.physics.add_crate(
                    *id,
                    PropState {
                        pos: body.pos,
                        rot: *rot,
                        vel: body.vel.as_vec3().to_array(),
                    },
                ),
                (false, true) => self.physics.remove_prop(*id),
                _ => {}
            }
        }
        self.physics.step();
        for (e, id, _, _) in props {
            let Some(st) = self.physics.prop(id) else {
                continue;
            };
            let mut em = self.world.entity_mut(e);
            if let Some(mut m) = em.get_mut::<Motion>() {
                m.set_if_neq(Motion(Body {
                    pos: st.pos,
                    vel: DVec3::new(st.vel[0] as f64, st.vel[1] as f64, st.vel[2] as f64),
                    grounded: false,
                }));
            }
            if let Some(mut o) = em.get_mut::<Orientation>() {
                o.set_if_neq(Orientation(st.rot));
            }
        }
    }

    /// Spawns a crate (centre `at`, turned by `rot`) and returns its ID.
    pub fn spawn_crate(&mut self, at: DVec3, rot: [f32; 4]) -> NetId {
        let id = self.world.resource_mut::<Ids>().0.allocate();
        self.world.spawn((
            Net(id),
            Motion(Body {
                pos: at,
                vel: DVec3::ZERO,
                grounded: false,
            }),
            Orientation(rot),
            Prop,
            LastChange(self.tick),
        ));
        id
    }

    /// Removes every prop (before restoring a save).
    pub fn clear_props(&mut self) {
        let found: Vec<(Entity, NetId)> = self
            .world
            .query_filtered::<(Entity, &Net), With<Prop>>()
            .iter(&self.world)
            .map(|(e, n)| (e, n.0))
            .collect();
        for (e, id) in found {
            self.physics.remove_prop(id);
            self.world.despawn(e);
        }
    }

    /// A pyramid of crates on the street just past where heroes appear,
    /// something to shove around (and the first thing physics has to hold up).
    pub fn place_starter_crates(&mut self) {
        let (mid, dir) = self.spawn;
        let side = dir.perp();
        let base = mid + dir * 18.0;
        let size = CRATE_HALF * 2.0 + 0.04;
        for (row, count) in [3u32, 2, 1].into_iter().enumerate() {
            for k in 0..count {
                let offset = (k as f64 - (count as f64 - 1.0) * 0.5) * size;
                let p = base + side * offset;
                let ground = self.world.resource::<Solids>().0.ground_at(p).max(GROUND_Y);
                let y = ground + CRATE_HALF + 0.02 + row as f64 * size;
                self.spawn_crate(DVec3::new(p.x, y, p.y), hw_net::protocol::NO_ROTATION);
            }
        }
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
            LastChange(self.tick),
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

    /// Moves an entity (spawning, teleports, tests).
    pub fn set_body(&mut self, id: NetId, body: Body) {
        for (n, mut m) in self
            .world
            .query::<(&Net, &mut Motion)>()
            .iter_mut(&mut self.world)
        {
            if n.0 == id {
                m.0 = body;
            }
        }
    }

    pub fn position(&mut self, id: NetId) -> Option<DVec3> {
        self.body(id).map(|b| b.pos)
    }

    /// Every replicated entity with the tick it last changed, sorted by ID.
    pub fn replicated(&mut self) -> Vec<(EntityState, u64)> {
        let mut out: Vec<(EntityState, u64)> = self
            .world
            .query::<(
                &Net,
                &Motion,
                Option<&Hero>,
                Option<&Orientation>,
                Has<Prop>,
                &LastChange,
            )>()
            .iter(&self.world)
            .map(|(n, m, h, o, prop, c)| {
                (
                    EntityState {
                        id: n.0,
                        kind: if prop {
                            EntityKind::Crate
                        } else {
                            EntityKind::Hero
                        },
                        pos: m.0.pos.to_array(),
                        vel: m.0.vel.as_vec3().to_array(),
                        rot: o.map_or(hw_net::protocol::NO_ROTATION, |o| o.0),
                        grounded: m.0.grounded,
                        owner: h.map(|h| h.owner),
                    },
                    c.0,
                )
            })
            .collect();
        out.sort_by_key(|(e, _)| e.id);
        out
    }

    /// The state of every replicated entity, sorted by ID.
    pub fn snapshot(&mut self) -> Vec<EntityState> {
        self.replicated().into_iter().map(|(e, _)| e).collect()
    }
}

fn move_heroes(solids: Res<Solids>, mut q: Query<(&Control, &mut Motion), With<Hero>>) {
    for (control, mut motion) in &mut q {
        let mut body = motion.0;
        step_hero(&mut body, control.0, &solids.0);
        // Only a real change counts, so heroes standing still cost no traffic.
        motion.set_if_neq(Motion(body));
    }
}

/// Entities whose replicated state changed since the last tick.
type Moved = Or<(Changed<Motion>, Changed<Orientation>)>;

fn track_changes(now: Res<Now>, mut q: Query<&mut LastChange, Moved>) {
    for mut c in &mut q {
        c.0 = now.0;
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
                descend: false,
                ..Default::default()
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
    fn heroes_stand_and_run_on_the_town_hills() {
        let town = GeneratedTown::generate(Seed(10), 3000.0);
        let mut sim = Sim::for_town(Seed(10), &town);
        let p = PlayerId(1);
        let id = sim.spawn_hero(p, sim.spawn_point(1));
        for _ in 0..TICK_HZ {
            sim.step();
        }
        let ground = |q: DVec3| town.world.ground(DVec2::new(q.x, q.z));
        let at = sim.position(id).unwrap();
        assert!(sim.body(id).unwrap().grounded);
        assert!(
            (at.y - ground(at)).abs() < 0.3,
            "{} vs {}",
            at.y,
            ground(at)
        );
        // Run for a few seconds: never sink into the hill or float off it.
        sim.set_input(
            p,
            InputState {
                move_x: 1.0,
                ..Default::default()
            },
        );
        for _ in 0..TICK_HZ * 4 {
            sim.step();
            let q = sim.position(id).unwrap();
            assert!(q.y >= ground(q).max(0.0) - 0.05, "sank at {q}");
        }
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
                            descend: false,
                            ..Default::default()
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
