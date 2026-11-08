//! The game server. It owns the truth: every entity, every hit point, every
//! broken wall. Clients (including the local one in single player) only show
//! and predict what this crate decides.
//!
//! The state lives in a `bevy_ecs` world and advances in fixed ticks
//! ([`TICK_HZ`]). [`host::Host`] puts a network socket around it.

pub mod alarms;
pub mod balance;
pub mod bubbles;
pub mod citizens;
pub mod collision;
pub mod combat;
pub mod destruction;
pub mod factions;
pub mod host;
pub mod incidents;
pub mod nav;
pub mod physics;
pub mod predict;
pub mod progress;
pub mod props;
pub mod quarter;
pub mod robots;
pub mod save;
pub mod streets;
pub mod taxi;
pub mod traffic;
pub mod wrecks;

use bevy_ecs::prelude::*;
use bubbles::Bubbles;
use collision::{Body, Obstacles, BODY_HEIGHT};
use combat::{Carried, CombatView, Drone, Enemy, Fighter, Fights, Kit, Rubble, Spot};
use destruction::Damage;
use glam::{DVec2, DVec3};
use hw_core::{NetId, NetIdAllocator, Seed};
use hw_gen::building::BuildingPlan;
use hw_gen::city::{CityParams, CityPlan};
use hw_gen::urban::world::TownWorld;
use hw_gen::urban::TownParams;
use hw_net::{
    EntityKind, EntityState, Fx, FxKind, ImpactKind, InputState, PlayerId, Power, Sheet, Status,
    ToClient,
};
use physics::{Physics, PropState, CRATE_HALF};
use std::collections::{BTreeMap, BTreeSet};
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

/// No gang hangs out this near where heroes appear (m).
pub const HANGOUT_CLEAR: f64 = 300.0;

/// A hero's progress (karma, level, powers): the player's hero file, which
/// the server plays by and sends back when it changes.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Progress {
    pub sheet: Sheet,
    /// Changed since the player was last told.
    pub changed: bool,
}

/// When a hero may punch or blast again (a tick).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Strike {
    pub ready_at: u64,
}

/// Ticks between punches, and between blasts.
pub const PUNCH_TICKS: u64 = 10;
pub const BLAST_TICKS: u64 = 18;
/// How far an energy blast reaches (m).
pub const BLAST_RANGE: f64 = 90.0;

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

/// The street where heroes appear (its middle and direction); the hospital
/// sends heroes back here for now.
#[derive(Resource, Clone, Copy)]
struct SpawnLine(DVec2, DVec2);

impl SpawnLine {
    fn hospital(&self, solids: &Obstacles) -> DVec3 {
        let p = self.0 - self.1 * 4.0;
        DVec3::new(p.x, solids.ground_at(p).max(GROUND_Y) + 0.5, p.y)
    }
}

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

/// Take-off speed with Super Jump (about seven metres up), and how well a
/// super jumper steers in the air.
pub const SUPER_JUMP_SPEED: f64 = 16.5;
pub const SUPER_AIR_ACCEL: f64 = 30.0;
/// A super jumper's leaps carry them along faster than running.
pub const SUPER_LEAP_SPEED: f64 = 13.0;

/// What a hero's movement powers allow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Moves {
    /// Flight is unlocked.
    pub fly: bool,
    /// Super Jump is owned.
    pub super_jump: bool,
}

impl Moves {
    /// A hero without movement powers (and villains).
    pub const ON_FOOT: Moves = Moves {
        fly: false,
        super_jump: false,
    };
    /// Flight allowed (tests and tools that fly).
    pub const FLYER: Moves = Moves {
        fly: true,
        super_jump: false,
    };

    /// From the effect bits a hero's [`hw_net::CombatState`] carries.
    pub fn from_effects(effects: u8) -> Self {
        Self {
            fly: effects & hw_net::EFFECT_CAN_FLY != 0,
            super_jump: effects & hw_net::EFFECT_SUPER_JUMP != 0,
        }
    }
}

/// One tick of a hero's movement with flight allowed (tests and tools).
pub fn step_hero(body: &mut Body, input: InputState, solids: &Obstacles) {
    step_moving(body, input, solids, Moves::FLYER);
}

/// One tick of a hero's movement with what their powers allow. The server
/// and client prediction both run exactly this, so they agree.
pub fn step_moving(body: &mut Body, mut input: InputState, solids: &Obstacles, moves: Moves) {
    input.fly &= moves.fly;
    if input.fly {
        fly_step(body, input);
        solids.move_body(body, DT);
        if body.pos.y > FLY_CEILING {
            body.pos.y = FLY_CEILING;
            body.vel.y = body.vel.y.min(0.0);
        }
        return;
    }
    let top = if moves.super_jump && !body.grounded {
        SUPER_LEAP_SPEED
    } else {
        RUN_SPEED
    };
    let want = DVec3::new(input.move_x as f64, 0.0, input.move_z as f64) * top;
    let horizontal = DVec3::new(body.vel.x, 0.0, body.vel.z);
    let delta = want - horizontal;
    let accel = if body.grounded {
        RUN_ACCEL
    } else if moves.super_jump {
        SUPER_AIR_ACCEL
    } else {
        AIR_ACCEL
    };
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
            body.vel.y = if moves.super_jump {
                SUPER_JUMP_SPEED
            } else {
                JUMP_SPEED
            };
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
    /// Broken buildings (see [`destruction`]).
    pub damage: Damage,
    /// Broken street props (see [`props`]).
    pub street: props::PropDamage,
    /// Taxi stands and the taxis flying fares (see [`taxi`]).
    pub taxis: taxi::Taxis,
    /// The moving traffic and the cars wrecked in it (see [`wrecks`]).
    pub wrecks: wrecks::Wrecks,
    /// Trouble that comes to the heroes (see [`incidents`]).
    pub incidents: incidents::Incidents,
    /// Trouble bystanders notice: they run, stare, cars stop (see
    /// [`alarms`]).
    pub alarms: alarms::Alarms,
    /// The alarm each open crime keeps up, by incident.
    crime_alarms: BTreeMap<u32, u32>,
}

impl Sim {
    /// A simulation of the city generated from `seed`.
    pub fn new(seed: Seed) -> Self {
        Self::for_city(seed, &GeneratedCity::generate(seed))
    }

    /// A simulation of a city already generated (the client shares its copy).
    pub fn for_city(seed: Seed, city: &GeneratedCity) -> Self {
        let mut sim = Self::with_solids(seed, city.obstacles.clone());
        sim.set_spawn(city.spawn_line());
        sim
    }

    /// A simulation of a town already generated (the client shares its copy).
    pub fn for_town(seed: Seed, town: &GeneratedTown) -> Self {
        let mut sim = Self::with_solids(seed, town.obstacles.clone());
        sim.set_spawn(town.world.spawn_line());
        sim.town_population = town.population();
        sim.street = props::PropDamage::new(town.world.clone()).with_solids(town.obstacles.clone());
        sim.taxis = taxi::Taxis::new(town.world.clone());
        sim.wrecks = wrecks::Wrecks::new(traffic::Traffic::new(town.world.clone()));
        sim.incidents = incidents::Incidents::new(town.world.clone());
        sim.world.resource_mut::<Fights>().streets = Some(Arc::new(streets::Streets::new(
            &town.world,
            streets::TANK_STREET,
        )));
        let solids = sim.solids();
        let corners = quarter::corners(&town.world);
        for (k, c) in corners.iter().enumerate() {
            let y = solids.ground_at(c.at).max(GROUND_Y);
            let i = sim.add_villain_corner(
                DVec3::new(c.at.x, y, c.at.y),
                c.facing,
                c.extra,
                quarter::WAKE,
            );
            // A robo-sentry guards the tower's gate and some corners; a
            // mutant alpha leads others.
            let lead = match k % 4 {
                0 => Some(combat::EnemyKind::Sentry),
                2 => Some(combat::EnemyKind::Alpha),
                _ => None,
            };
            sim.world.resource_mut::<Fights>().spots[i].lead = lead;
        }
        for l in quarter::lookouts(&town.world, &corners) {
            if let Some(p) = quarter::perch(&solids, &l) {
                sim.add_lookout(p, l.facing);
            }
        }
        // The Chrome Jackets' corners in the band round the quarter, never
        // where heroes appear.
        for c in quarter::hangouts(&town.world, sim.spawn.0, HANGOUT_CLEAR) {
            let y = solids.ground_at(c.at).max(GROUND_Y);
            sim.add_hangout(DVec3::new(c.at.x, y, c.at.y), c.facing);
        }
        sim
    }

    /// A Chrome Jackets hangout: a few greasers on a corner, at the team's
    /// level, while a hero is near; they warn strangers off. Returns the
    /// spot's index.
    pub fn add_hangout(&mut self, at: DVec3, facing: DVec2) -> usize {
        let i = self.add_villain_corner(at, facing, 0, quarter::WAKE);
        let mut f = self.world.resource_mut::<Fights>();
        let spot = &mut f.spots[i];
        spot.only = Some(combat::EnemyKind::Greaser);
        spot.size = Some(quarter::HANGOUT_GROUP);
        i
    }

    /// Breaks the first `n` weak points (knees, then the cannon) of every
    /// colossus that still has them standing (screenshots, tests).
    pub fn break_colossus(&mut self, n: u8) {
        let tanks: Vec<NetId> = self
            .world
            .query::<(&Net, &Enemy)>()
            .iter(&self.world)
            .filter(|(_, e)| e.kind == combat::EnemyKind::Colossus)
            .map(|(n, _)| n.0)
            .collect();
        for (m, mut f) in self
            .world
            .query::<(&robots::Mount, &mut Fighter)>()
            .iter_mut(&mut self.world)
        {
            if tanks.contains(&m.on) && m.part < n && f.standing() {
                f.health = 0.0;
                f.status = Status::Down;
                f.until = self.tick;
            }
        }
    }

    /// A colossus attack: the walking tank at `at`, marching along `facing`
    /// (tests and screenshots; in the game it is an incident). Returns its ID.
    pub fn add_colossus(&mut self, at: DVec3, facing: DVec2) -> NetId {
        let i = {
            let mut f = self.world.resource_mut::<Fights>();
            f.spots.push(Spot {
                size: Some(1),
                only: Some(combat::EnemyKind::Colossus),
                once: true,
                ..Spot::fight(at, facing)
            });
            f.spots.len() - 1
        };
        let heroes = self.world.query::<&Hero>().iter(&self.world).count();
        combat::spawn_group(&mut self.world, self.tick, i, heroes.max(1));
        self.world
            .query::<(&Net, &Enemy)>()
            .iter(&self.world)
            .filter(|(_, e)| e.kind == combat::EnemyKind::Colossus && e.spot == i)
            .map(|(n, _)| n.0)
            .next()
            .expect("the colossus was made")
    }

    /// A simulation with the given solids (tests use flat ground).
    pub fn with_solids(seed: Seed, solids: Arc<Obstacles>) -> Self {
        let mut world = World::new();
        world.insert_resource(Ids(NetIdAllocator::new()));
        world.insert_resource(Solids(solids.clone()));
        world.insert_resource(Now(0));
        world.insert_resource(SpawnLine(DVec2::ZERO, DVec2::X));
        world.insert_resource(Fights {
            seed: seed.0,
            ..Default::default()
        });
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
            damage: Damage::default(),
            street: props::PropDamage::default(),
            taxis: taxi::Taxis::default(),
            wrecks: wrecks::Wrecks::default(),
            incidents: incidents::Incidents::default(),
            alarms: alarms::Alarms::default(),
            crime_alarms: BTreeMap::new(),
        }
    }

    /// People near a crime notice it: a mugging or a robbery while it
    /// lasts, a rampage or a breakout as a fight; a gang's shout too.
    fn raise_crime_alarms(&mut self) {
        use alarms::AlarmKind;
        use incidents::IncidentKind;
        let tick = self.tick;
        let warnings: Vec<DVec3> = self
            .world
            .resource::<Fights>()
            .events
            .iter()
            .filter(|e| e.kind == FxKind::Warn)
            .map(|e| DVec3::from_array(e.at.map(|x| x as f64)))
            .collect();
        for at in warnings {
            self.alarms
                .raise_or_refresh(AlarmKind::Warning, at, tick, TICK_HZ);
        }
        if !tick.is_multiple_of(TICK_HZ as u64) {
            return;
        }
        let open: Vec<(u32, AlarmKind, DVec3)> = self
            .incidents
            .open
            .iter()
            .filter_map(|i| {
                let kind = match i.kind {
                    IncidentKind::Mugging => AlarmKind::Mugging,
                    IncidentKind::Robbery => AlarmKind::Robbery,
                    IncidentKind::Rampage | IncidentKind::Breakout => AlarmKind::Fight,
                    _ => return None,
                };
                Some((i.id, kind, i.at))
            })
            .collect();
        for &(id, kind, at) in &open {
            match self.crime_alarms.get(&id) {
                Some(&a) => self.alarms.refresh(a, at, tick, TICK_HZ),
                None => {
                    let a = self.alarms.raise(kind, at, tick, TICK_HZ);
                    self.crime_alarms.insert(id, a);
                }
            }
        }
        let over: Vec<u32> = self
            .crime_alarms
            .keys()
            .copied()
            .filter(|id| open.iter().all(|o| o.0 != *id))
            .collect();
        for id in over {
            if let Some(a) = self.crime_alarms.remove(&id) {
                self.alarms.end(a);
            }
        }
    }

    /// Sets the street heroes appear on.
    pub fn set_spawn(&mut self, line: (DVec2, DVec2)) {
        self.spawn = line;
        self.world.insert_resource(SpawnLine(line.0, line.1));
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
        combat::step(&mut self.world, self.tick);
        self.machines_break_the_city();
        self.incidents.step(&mut self.world, self.tick);
        self.raise_crime_alarms();
        let solids = self.solids();
        taxi::step(&mut self.world, &mut self.taxis, &solids, self.tick, DT);
        self.alarms.step(self.tick);
        // Cars stop for anyone in the road (heroes, villains on their feet
        // or knocked down, a mugging's victim, someone lying hurt), and
        // short of a fight or a collapse.
        let mut standing: Vec<DVec3> = self
            .world
            .query::<(
                &Motion,
                Option<&Hero>,
                Option<&Fighter>,
                Has<incidents::Bystander>,
            )>()
            .iter(&self.world)
            .filter(|(_, h, f, b)| {
                h.is_some() || *b || f.is_some_and(|f| f.status != Status::Carried)
            })
            .map(|(m, ..)| m.0.pos)
            .collect();
        standing.extend(self.incidents.open.iter().filter_map(|i| match i.hurt {
            Some(incidents::Hurt::Lying(p)) => Some(p),
            _ => None,
        }));
        let stops = self.alarms.car_stops();
        if let Some(t) = self.wrecks.traffic.as_mut() {
            t.set_blockers(&standing, &stops);
            t.advance_to(self.tick as f64 * DT);
            // Heroes bump into the cars near them (and can stand on them).
            let heroes: Vec<DVec3> = self
                .world
                .query::<(&Motion, &Hero)>()
                .iter(&self.world)
                .map(|(m, _)| m.0.pos)
                .collect();
            let cars = t.car_solids(&heroes, 25.0, self.tick as f64 * DT);
            self.world.resource::<Solids>().0.set_movers(cars);
        }
        self.pay_karma();
        self.sync_levels();
        self.clear_rubble();
        let heroes: Vec<(PlayerId, NetId, DVec3)> = self
            .world
            .query::<(&Hero, &Net, &Motion)>()
            .iter(&self.world)
            .map(|(h, n, m)| (h.owner, n.0, m.0.pos))
            .collect();
        let players: Vec<(PlayerId, DVec3)> = heroes.iter().map(|(p, _, at)| (*p, *at)).collect();
        self.bubbles.update(&players);
        self.strike();
        if self.tick.is_multiple_of(TICK_HZ as u64) {
            let solids = self.solids();
            let at: Vec<DVec3> = heroes.iter().map(|h| h.2).collect();
            self.damage.repair_unseen(&solids, &at, self.tick, TICK_HZ);
            self.street.repair_unseen(&at, self.tick, TICK_HZ);
            self.wrecks.repair_unseen(&at, self.tick, TICK_HZ);
        }
        self.step_physics(&heroes);
        self.tick += 1;
        self.world.resource_mut::<Now>().0 = self.tick;
        self.track.run(&mut self.world);
    }

    /// The colossus's feet, hull and shells break what they hit.
    fn machines_break_the_city(&mut self) {
        let blows = std::mem::take(&mut self.world.resource_mut::<Fights>().city);
        if blows.is_empty() {
            return;
        }
        let solids = self.solids();
        for (at, radius, power) in blows {
            let (_, _) = self.street.hit(at, DVec3::Y, radius, power, self.tick);
            let (_, _) = self.wrecks.hit(at, DVec3::Y, radius, power, self.tick);
            let hit = self.damage.hit(&solids, at, radius, power, None, self.tick);
            let broke = hit.broken;
            self.alarm_for_damage(at, broke, !hit.collapsed.is_empty());
            if broke > 0 {
                self.damage.tell(ToClient::Impact {
                    at: at.as_vec3().to_array(),
                    dir: [0.0, -1.0, 0.0],
                    kind: ImpactKind::Blast,
                    broke: true,
                });
            }
        }
    }

    /// Heroes punch and blast what is in front of them.
    fn strike(&mut self) {
        let tick = self.tick;
        let mut blows = Vec::new();
        let mut fights = Vec::new();
        for (hero, control, motion, mut strike, fighter) in self
            .world
            .query::<(&Hero, &Control, &Motion, &mut Strike, Option<&Fighter>)>()
            .iter_mut(&mut self.world)
        {
            let input = control.0;
            // Blasting needs Energy Bolt.
            let blast = input.blast && fighter.is_none_or(|f| f.kit.has(Power::Bolt));
            if tick < strike.ready_at || !(input.smash || blast) {
                continue;
            }
            // A blow at a villain is the fight's (see `combat`); the downed
            // break nothing.
            if fighter.is_some_and(|f| f.engaged) {
                fights.push(motion.0.pos);
            }
            if fighter.is_some_and(|f| f.engaged || !f.standing()) {
                continue;
            }
            let aim = DVec3::new(
                input.aim[0] as f64,
                input.aim[1] as f64,
                input.aim[2] as f64,
            )
            .normalize_or(DVec3::Z);
            let chest = motion.0.pos + DVec3::Y * (BODY_HEIGHT * 0.7);
            // Level sets how hard heroes hit, so what they can break.
            let level = fighter.map_or(1, |f| f.level);
            if blast {
                strike.ready_at = tick + BLAST_TICKS;
                blows.push((hero.owner, ImpactKind::Blast, chest, aim, level));
            } else {
                strike.ready_at = tick + PUNCH_TICKS;
                blows.push((hero.owner, ImpactKind::Punch, chest, aim, level));
            }
        }
        // Bystanders notice a fight.
        for at in fights {
            self.alarms
                .raise_or_refresh(alarms::AlarmKind::Fight, at, tick, TICK_HZ);
        }
        if blows.is_empty() {
            return;
        }
        let solids = self.solids();
        for (owner, kind, chest, aim, level) in blows {
            let power = destruction::hero_power(level);
            let (at, radius, strength) = match kind {
                ImpactKind::Punch => (chest + aim * 0.9, power.punch_radius, power.punch),
                ImpactKind::Blast => {
                    let wall = destruction::raycast(&solids, chest, aim, BLAST_RANGE);
                    let reach = wall.map_or(BLAST_RANGE, |p| p.distance(chest));
                    // A lamp post or car in the way takes the blast first.
                    let prop = self.street.raycast(chest, aim, reach);
                    let reach = prop.map_or(reach, |p| p.distance(chest));
                    let car = self.wrecks.raycast(chest, aim, reach, tick);
                    let Some(p) = car.or(prop).or(wall) else {
                        continue;
                    };
                    (p, power.blast_radius, power.blast)
                }
            };
            let (props, spent) = self.street.hit(at, aim, radius, strength, tick);
            props::charge(&mut self.damage.collateral, Some(owner), spent);
            let (cars, spent) = self.wrecks.hit(at, aim, radius, strength, tick);
            props::charge(&mut self.damage.collateral, Some(owner), spent);
            let props = props + cars;
            // A punch at thin air hits nothing (a prop it broke counts).
            let solid = solids.solid_at(at) || solids.solid_at(at + aim * 0.4);
            let broken = if kind == ImpactKind::Blast || solid {
                let hit = self
                    .damage
                    .hit(&solids, at, radius, strength, Some(owner), tick);
                self.alarm_for_damage(at, hit.broken + props, !hit.collapsed.is_empty());
                hit.broken
            } else {
                0
            };
            if kind == ImpactKind::Punch && !solid && props == 0 {
                continue;
            }
            self.damage.tell(ToClient::Impact {
                at: at.as_vec3().to_array(),
                dir: aim.as_vec3().to_array(),
                kind,
                broke: broken + props > 0,
            });
        }
    }

    /// Walls and props smashed at `at` (`broke` pieces; a building
    /// `collapsed`): people near run.
    fn alarm_for_damage(&mut self, at: DVec3, broke: usize, collapsed: bool) {
        let kind = if collapsed {
            alarms::AlarmKind::Collapse
        } else if broke > 0 {
            alarms::AlarmKind::Explosion
        } else {
            return;
        };
        self.alarms.raise_or_refresh(kind, at, self.tick, TICK_HZ);
    }

    /// Reliable news for every client since the last call: damage, repairs,
    /// impacts.
    pub fn take_news(&mut self) -> Vec<ToClient> {
        let mut news = self.alarms.take_news();
        news.extend(self.street.take_news());
        news.extend(self.wrecks.take_news());
        news.extend(self.incidents.take_news());
        news.extend(self.damage.take_news());
        news
    }

    fn step_physics(&mut self, heroes: &[(PlayerId, NetId, DVec3)]) {
        // Heroes and standing villains shove crates about.
        let mut people: Vec<(NetId, DVec3)> = heroes.iter().map(|(_, id, at)| (*id, *at)).collect();
        people.extend(
            self.world
                .query_filtered::<(&Net, &Motion, &Fighter), With<Enemy>>()
                .iter(&self.world)
                .filter(|(_, _, f)| f.standing())
                .map(|(n, m, _)| (n.0, m.0.pos)),
        );
        let keep: BTreeSet<NetId> = people.iter().map(|(id, _)| *id).collect();
        self.physics.keep_heroes(&keep);
        for (id, at) in people {
            self.physics.place_hero(id, at);
        }
        // Props inside a bubble are simulated, nearest the bubble's centre
        // first, up to its budget; the rest hold still where they are.
        // Lifted things ride with their heroes, outside physics.
        let carried: Vec<NetId> = self
            .world
            .query_filtered::<&Net, (With<Prop>, With<Carried>)>()
            .iter(&self.world)
            .map(|n| n.0)
            .collect();
        for id in carried {
            if self.physics.is_awake(id) {
                self.physics.remove_prop(id);
            }
        }
        let mut props: Vec<(Entity, NetId, Body, [f32; 4])> = self
            .world
            .query_filtered::<(Entity, &Net, &Motion, &Orientation), (With<Prop>, Without<Carried>)>()
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

    /// Spawns a hero for `owner` at `at` and returns its ID: a starter hero
    /// (Strike and Energy Bolt) until the player's hero file arrives
    /// ([`Sim::set_sheet`]).
    pub fn spawn_hero(&mut self, owner: PlayerId, at: DVec3) -> NetId {
        self.spawn_hero_with(owner, at, progress::starter())
    }

    /// Spawns a hero for `owner` with the progress `sheet`.
    pub fn spawn_hero_with(&mut self, owner: PlayerId, at: DVec3, sheet: Sheet) -> NetId {
        let id = self.world.resource_mut::<Ids>().0.allocate();
        let sheet = progress::sanitize(sheet);
        let level = self.world.resource::<Fights>().team_level;
        let fighter = Fighter::hero_at(level, Kit::of(&sheet));
        self.world.spawn((
            Progress {
                sheet,
                changed: true,
            },
            Net(id),
            Motion(Body {
                pos: at,
                vel: DVec3::ZERO,
                grounded: false,
            }),
            Hero { owner },
            Control::default(),
            Strike::default(),
            fighter,
            CombatView(fighter.view(self.tick)),
            LastChange(self.tick),
        ));
        id
    }

    /// A group of the mad scientist's minions `at`, facing `facing`, that
    /// fights any hero it notices (tests, the balance runs); it stands
    /// there at once and comes back. Returns the spot's index.
    pub fn add_fight_spot(&mut self, at: DVec3, facing: DVec2) -> usize {
        let i = {
            let mut f = self.world.resource_mut::<Fights>();
            f.spots.push(Spot::fight(at, facing));
            f.spots.len() - 1
        };
        let heroes = self.world.query::<&Hero>().iter(&self.world).count();
        combat::spawn_group(&mut self.world, self.tick, i, heroes.max(1));
        i
    }

    /// An ambient corner (in the mad scientist's quarter): a group `extra`
    /// levels above the team stands there while a hero is within `wake` m.
    /// Returns the spot's index.
    pub fn add_villain_corner(&mut self, at: DVec3, facing: DVec2, extra: u8, wake: f64) -> usize {
        let mut f = self.world.resource_mut::<Fights>();
        f.spots.push(Spot {
            respawn_at: Some(0),
            extra,
            wake: Some(wake),
            ..Spot::corner(at, facing)
        });
        f.spots.len() - 1
    }

    /// A rooftop lookout: technicians up on the roof at `at` (its top),
    /// watching the street that way, [`quarter::CORNER_EXTRA`] levels above
    /// the team, while a hero is near. Returns the spot's index.
    pub fn add_lookout(&mut self, at: DVec3, facing: DVec2) -> usize {
        let i = self.add_villain_corner(at, facing, quarter::CORNER_EXTRA, quarter::WAKE);
        let mut f = self.world.resource_mut::<Fights>();
        let spot = &mut f.spots[i];
        // They stand in a line across the roof's middle (technicians form
        // up 3.5 m behind a spot's centre).
        spot.center += DVec3::new(spot.facing.x, 0.0, spot.facing.y) * 3.5;
        spot.perch = true;
        spot.only = Some(combat::EnemyKind::Technician);
        spot.size = Some(quarter::ROOF_GROUP);
        i
    }

    /// A test fight: a group of minions on the street, the other way from
    /// the crates, a short run from where heroes appear (tests and the
    /// fight screenshot; the game starts without it).
    pub fn place_first_fight(&mut self) -> usize {
        let (mid, dir) = self.spawn;
        let p = mid - dir * 32.0;
        let ground = self.world.resource::<Solids>().0.ground_at(p).max(GROUND_Y);
        self.add_fight_spot(DVec3::new(p.x, ground, p.y), dir)
    }

    /// The encounter spots and what happened in the last tick.
    pub fn fights(&self) -> &Fights {
        self.world.resource::<Fights>()
    }

    /// What happened in fights during the last tick.
    pub fn fx(&self) -> &[Fx] {
        &self.world.resource::<Fights>().events
    }

    /// A fighter's state (health, energy, status, powers).
    pub fn fighter(&mut self, id: NetId) -> Option<Fighter> {
        self.world
            .query::<(&Net, &Fighter)>()
            .iter(&self.world)
            .find(|(n, _)| n.0 == id)
            .map(|(_, f)| *f)
    }

    /// Changes a fighter (tests, cheats).
    pub fn set_fighter(&mut self, id: NetId, fighter: Fighter) {
        for (n, mut f) in self
            .world
            .query::<(&Net, &mut Fighter)>()
            .iter_mut(&mut self.world)
        {
            if n.0 == id {
                *f = fighter;
            }
        }
    }

    /// Villains (and drones) in the world, by ID: (id, kind, position, fighter).
    pub fn enemies(&mut self) -> Vec<(NetId, combat::EnemyKind, DVec3, Fighter)> {
        let mut v: Vec<_> = self
            .world
            .query::<(&Net, &Motion, &Enemy, &Fighter)>()
            .iter(&self.world)
            .map(|(n, m, e, f)| (n.0, e.kind, m.0.pos, *f))
            .collect();
        v.sort_by_key(|x| x.0);
        v
    }

    /// The progress of `owner`'s hero.
    pub fn sheet(&mut self, owner: PlayerId) -> Option<Sheet> {
        self.world
            .query::<(&Hero, &Progress)>()
            .iter(&self.world)
            .find(|(h, _)| h.owner == owner)
            .map(|(_, p)| p.sheet.clone())
    }

    /// Plays `owner`'s hero by the hero file `sheet` (sent when joining).
    pub fn set_sheet(&mut self, owner: PlayerId, sheet: Sheet) {
        let sheet = progress::sanitize(sheet);
        self.change_sheet(owner, |s| *s = sheet);
    }

    /// `owner` buys `power` in `theme`. Returns the price paid.
    pub fn buy(
        &mut self,
        owner: PlayerId,
        power: Power,
        theme: u8,
    ) -> Result<u32, progress::BuyError> {
        let mut result = Err(progress::BuyError::NotForSale);
        self.change_sheet(owner, |s| result = progress::buy(s, power, theme));
        result
    }

    /// `owner` gives `power` up. Returns the karma refunded.
    pub fn give_up(&mut self, owner: PlayerId, power: Power) -> Option<u32> {
        let mut back = None;
        self.change_sheet(owner, |s| back = progress::give_up(s, power));
        back
    }

    fn change_sheet(&mut self, owner: PlayerId, f: impl FnOnce(&mut Sheet)) {
        let mut f = Some(f);
        for (hero, mut p, mut fighter) in self
            .world
            .query::<(&Hero, &mut Progress, &mut Fighter)>()
            .iter_mut(&mut self.world)
        {
            if hero.owner != owner {
                continue;
            }
            let before = p.sheet.clone();
            if let Some(f) = f.take() {
                f(&mut p.sheet);
            }
            if p.sheet != before {
                p.changed = true;
                fighter.kit = Kit::of(&p.sheet);
                // A power given up lets go of what it held.
                if !fighter.kit.has(Power::Lift) {
                    fighter.carrying = None;
                }
            }
        }
        self.sync_levels();
    }

    /// Hero files that changed since the last call, for their players.
    pub fn take_sheets(&mut self) -> Vec<(PlayerId, Sheet)> {
        let mut out: Vec<(PlayerId, Sheet)> = self
            .world
            .query::<(&Hero, &mut Progress)>()
            .iter_mut(&mut self.world)
            .filter(|(_, p)| p.changed)
            .map(|(h, mut p)| {
                p.changed = false;
                (h.owner, p.sheet.clone())
            })
            .collect();
        out.sort_by_key(|(p, _)| *p);
        out
    }

    /// The level everyone fights at: the host's hero's (the lowest player
    /// number present), so friends of different levels fight together.
    pub fn team_level(&self) -> u8 {
        self.world.resource::<Fights>().team_level
    }

    /// Pays out the karma fights earned this tick; a hero going up a level
    /// is healed and told.
    fn pay_karma(&mut self) {
        // Factions hear who beat their people.
        let deeds = self.world.resource::<Fights>().deeds.clone();
        for d in deeds {
            for (n, mut p) in self
                .world
                .query::<(&Net, &mut Progress)>()
                .iter_mut(&mut self.world)
            {
                if n.0 == d.hero {
                    factions::beat(&mut p.sheet.regard, d.beaten);
                    p.changed = true;
                }
            }
        }
        let rewards = self.world.resource::<Fights>().rewards.clone();
        if rewards.is_empty() {
            return;
        }
        let tick = self.tick;
        let mut fx = Vec::new();
        for r in rewards {
            for (n, mut p, mut fighter, m) in self
                .world
                .query::<(&Net, &mut Progress, &mut Fighter, &Motion)>()
                .iter_mut(&mut self.world)
            {
                if n.0 != r.hero {
                    continue;
                }
                let up = progress::earn(&mut p.sheet, r.karma, r.deed);
                if let Some(a) = r.deed {
                    let x = &mut p.sheet.affinity[a.index()];
                    *x = x.saturating_add(r.lean);
                }
                p.changed = true;
                let at = m.0.pos + DVec3::Y * 1.2;
                if r.karma > 0 {
                    let mut e = combat_event(tick, FxKind::Karma, n.0, at);
                    e.amount = r.karma.min(u16::MAX as u32) as u16;
                    fx.push(e);
                }
                if let Some(level) = up {
                    fighter.health = fighter.max_health;
                    fighter.energy = fighter.max_energy;
                    let mut e = combat_event(tick, FxKind::LevelUp, n.0, at);
                    e.amount = level as u16;
                    fx.push(e);
                }
            }
        }
        self.world.resource_mut::<Fights>().events.extend(fx);
    }

    /// Everyone fights at the host's hero's level: health, damage and new
    /// villain groups follow it.
    fn sync_levels(&mut self) {
        let lead = self
            .world
            .query::<(&Hero, &Progress)>()
            .iter(&self.world)
            .min_by_key(|(h, _)| h.owner)
            .map(|(_, p)| progress::level_of(p.sheet.lifetime));
        let Some(level) = lead else {
            return;
        };
        self.world.resource_mut::<Fights>().team_level = level;
        for mut f in self
            .world
            .query_filtered::<&mut Fighter, With<Hero>>()
            .iter_mut(&mut self.world)
        {
            if f.level != level {
                f.set_level(level);
            }
        }
    }

    /// Rubble that was thrown a while ago is swept away.
    fn clear_rubble(&mut self) {
        let tick = self.tick;
        let gone: Vec<(Entity, NetId)> = self
            .world
            .query::<(Entity, &Net, &Rubble)>()
            .iter(&self.world)
            .filter(|(_, _, r)| r.gone_at.is_some_and(|t| tick >= t))
            .map(|(e, n, _)| (e, n.0))
            .collect();
        for (e, id) in gone {
            self.physics.remove_prop(id);
            self.world.despawn(e);
        }
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

    /// `owner` hails a taxi to stand `dest` (see [`taxi::Taxis::stands`]),
    /// or with [`taxi::HOP_OUT`] gets out of it.
    pub fn hail_taxi(&mut self, owner: PlayerId, dest: u8) {
        let Some(hero) = self
            .world
            .query::<(Entity, &Hero)>()
            .iter(&self.world)
            .find(|(_, h)| h.owner == owner)
            .map(|(e, _)| e)
        else {
            return;
        };
        let id = self.world.resource_mut::<Ids>().0.allocate();
        let solids = self.solids();
        taxi::hail(
            &mut self.world,
            &mut self.taxis,
            id,
            &solids,
            owner,
            hero,
            dest,
            self.tick,
        );
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
                (Has<Prop>, Option<&Enemy>, Has<Drone>, Option<&CombatView>),
                (
                    &LastChange,
                    Option<&taxi::Fare>,
                    Option<&robots::Mount>,
                    Has<incidents::Bystander>,
                ),
            )>()
            .iter(&self.world)
            .map(
                |(n, m, h, o, (prop, enemy, drone, view), (c, fare, mount, bystander))| {
                    (
                        EntityState {
                            id: n.0,
                            kind: if prop {
                                EntityKind::Crate
                            } else if fare.is_some() {
                                EntityKind::Taxi
                            } else if let Some(e) = enemy {
                                e.kind.entity_kind()
                            } else if drone {
                                EntityKind::Drone
                            } else if bystander {
                                EntityKind::Bystander
                            } else {
                                EntityKind::Hero
                            },
                            combat: view.map(|v| v.0),
                            pos: m.0.pos.to_array(),
                            vel: m.0.vel.as_vec3().to_array(),
                            rot: o.map_or(hw_net::protocol::NO_ROTATION, |o| o.0),
                            grounded: m.0.grounded,
                            owner: h.map(|h| h.owner).or(fare.map(|f| f.0)),
                            mount: mount.map(|m| (m.on, m.part)),
                        },
                        c.0,
                    )
                },
            )
            .collect();
        out.sort_by_key(|(e, _)| e.id);
        out
    }

    /// The state of every replicated entity, sorted by ID.
    pub fn snapshot(&mut self) -> Vec<EntityState> {
        self.replicated().into_iter().map(|(e, _)| e).collect()
    }
}

/// Heroes that move by their own input (not riding a taxi).
type WalkingHero = (With<Hero>, Without<taxi::Riding>);

fn move_heroes(
    solids: Res<Solids>,
    mut q: Query<(&Control, &mut Motion, Option<&Fighter>), WalkingHero>,
) {
    for (control, mut motion, fighter) in &mut q {
        let moves = fighter.map_or(Moves::ON_FOOT, |f| Moves {
            fly: f.kit.has(Power::Flight),
            super_jump: f.kit.has(Power::SuperJump),
        });
        let status = fighter.map_or(Status::Standing, |f| f.status);
        let input = match status {
            // A drone holds the carried; the downed only fall.
            Status::Carried => continue,
            Status::Down => InputState::default(),
            _ => control.0,
        };
        let mut body = motion.0;
        step_moving(&mut body, input, &solids.0, moves);
        // Only a real change counts, so heroes standing still cost no traffic.
        motion.set_if_neq(Motion(body));
    }
}

fn combat_event(tick: u64, kind: FxKind, who: NetId, at: DVec3) -> Fx {
    Fx {
        tick,
        kind,
        from: who,
        to: who,
        at: at.as_vec3().to_array(),
        amount: 0,
        knock: false,
        defeat: false,
        theme: 0,
    }
}

/// Entities whose replicated state changed since the last tick.
type Moved = Or<(Changed<Motion>, Changed<Orientation>, Changed<CombatView>)>;

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
