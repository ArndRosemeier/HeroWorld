//! The game server. It owns the truth: every entity, every hit point, every
//! broken wall. Clients (including the local one in single player) only show
//! and predict what this crate decides.
//!
//! The state lives in a `bevy_ecs` world and advances in fixed ticks
//! ([`TICK_HZ`]). [`host::Host`] puts a network socket around it.

pub mod airships;
pub mod alarms;
pub mod balance;
pub mod bubbles;
pub mod citizens;
pub mod clock;
pub mod collision;
pub mod combat;
pub mod crimes;
pub mod deepspots;
pub mod destruction;
pub mod doors;
pub mod factions;
pub mod floornav;
pub mod gates;
pub mod host;
pub mod hostthread;
pub mod incidents;
pub mod lifts;
pub mod missions;
pub mod nav;
pub mod npcheroes;
pub mod physics;
pub mod powers;
pub mod predict;
pub mod progress;
pub mod props;
pub mod quarter;
pub mod rainmaker;
pub mod robots;
pub mod rocket;
pub mod roosts;
pub mod save;
pub mod sidekick;
pub mod story;
pub mod streetlife;
pub mod streets;
pub mod taxi;
pub mod tiers;
pub mod traffic;
pub mod tube;
pub mod wrecks;

use bevy_ecs::prelude::*;
use bubbles::Bubbles;
use collision::{Body, Obstacles, BODY_HEIGHT};
use combat::{Carried, CombatView, Drone, Enemy, Fighter, Fights, Kit, Rubble, Spot};
use destruction::Damage;
use glam::{DVec2, DVec3};
use hw_core::{Cancel, Cancelled, NetId, NetIdAllocator, Seed};
use hw_gen::building::BuildingPlan;
use hw_gen::city::{CityParams, CityPlan};
use hw_gen::urban::area::{Area, AREA_HALF};
use hw_gen::urban::world::TownWorld;
use hw_gen::urban::TownParams;
use hw_net::{
    EntityKind, EntityState, Fx, FxKind, ImpactKind, InputState, PlayerId, PowerId, Sheet, Status,
    ToClient,
};
use physics::{Physics, PropState, CRATE_HALF};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Simulation ticks per second.
pub const TICK_HZ: u32 = 30;
/// What one part of the server's step may take in one tick (µs): a third
/// of the tick. More is logged as an overrun naming the part.
pub const PART_BUDGET_US: u64 = 12_000;
/// Most bodies the traffic stops for in one tick (see [`thin_out`]).
pub const MAX_BLOCKERS: usize = 120;

/// `standing` with at most one body per 2 m square and at most
/// [`MAX_BLOCKERS`], the ones nearest a hero first (all in a fixed order).
fn thin_out(mut standing: Vec<DVec3>, heroes: &[DVec3]) -> Vec<DVec3> {
    let cell = |p: &DVec3| ((p.x / 2.0).floor() as i64, (p.z / 2.0).floor() as i64);
    standing.sort_by(|a, b| cell(a).cmp(&cell(b)).then(a.y.total_cmp(&b.y)));
    standing.dedup_by(|a, b| cell(a) == cell(b) && (a.y - b.y).abs() < 2.5);
    if standing.len() > MAX_BLOCKERS {
        let near = |p: &DVec3| {
            heroes
                .iter()
                .map(|h| h.distance_squared(*p))
                .fold(f64::INFINITY, f64::min)
        };
        standing.sort_by(|a, b| near(a).total_cmp(&near(b)));
        standing.truncate(MAX_BLOCKERS);
    }
    standing
}

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
/// The pirates' landing is never this near where heroes appear (m).
pub const ROOST_CLEAR: f64 = 200.0;
/// Molehaven's guards stand at their gate while a hero is this near (m).
pub const MOLE_WAKE: f64 = 90.0;

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

/// The street where heroes appear (its middle and direction), and the one
/// new heroes start on; without a hospital (a bare test ground) the
/// medical drone brings heroes back to theirs.
#[derive(Resource, Clone, Copy)]
struct SpawnLine(DVec2, DVec2, Option<(DVec2, DVec2)>);

impl SpawnLine {
    /// Where a hero fighting at `level` comes back after the medical drone.
    fn hospital(&self, solids: &Obstacles, level: u8) -> DVec3 {
        let (mid, dir) = match self.2 {
            Some(rookie) if level < quarter::ROOKIE_LEVEL => rookie,
            _ => (self.0, self.1),
        };
        let p = mid - dir * 4.0;
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

/// The server has the town plan the ground this far round each hero ahead
/// of time (m).
pub const PREFETCH_REACH: f64 = 600.0;

/// The square with people and traffic moves when the heroes are this far
/// from its centre (m, along x or z): they are always at least
/// `AREA_HALF - AREA_SHIFT` from its edge.
pub const AREA_SHIFT: f64 = 1500.0;
/// Its centre snaps to a grid this fine (m).
const AREA_SNAP: f64 = 250.0;

/// The town every machine generates from the seed and its size, with its
/// collision.
pub struct GeneratedTown {
    pub world: Arc<TownWorld>,
    /// The square round the heroes' start with people, traffic, incidents
    /// and taxis (see [`hw_gen::urban::area`]).
    pub area: Arc<Area>,
    pub obstacles: Arc<Obstacles>,
}

impl GeneratedTown {
    /// The town of `seed` with about `population` people.
    pub fn generate(seed: Seed, population: f64) -> Self {
        Self::generate_or_stop(seed, population, &Cancel::new()).expect("nobody can cancel this")
    }

    /// [`GeneratedTown::generate`] that gives up between its steps once
    /// `cancel` is set (a city behind the title the player no longer
    /// wants). The same town when it is not cancelled.
    pub fn generate_or_stop(
        seed: Seed,
        population: f64,
        cancel: &Cancel,
    ) -> Result<Self, Cancelled> {
        let world = Arc::new(TownWorld::generate_or_stop(
            seed,
            TownParams::sample(seed, population),
            cancel,
        )?);
        cancel.check()?;
        cancel.stage("Finding the way in", 0.2, 0.2);
        let w = world.clone();
        // Open ground is what is drawn there: roads stand a few
        // centimetres above the terrain.
        let terrain = collision::Terrain(Arc::new(move |p| w.surface(p)));
        let (start, _) = world.spawn_line();
        // The collision below holds the underground: plan it where it can stop.
        world.underground_or_stop(cancel)?;
        let area = Arc::new(Area::around_or_stop(
            world.clone(),
            start,
            AREA_HALF,
            cancel,
        )?);
        cancel.check()?;
        cancel.stage("Placing the solid things", 0.97, 1.0);
        let obstacles = Arc::new(Obstacles::from_town(&world, terrain));
        cancel.check()?;
        Ok(Self {
            world,
            area,
            obstacles,
        })
    }

    pub fn population(&self) -> f64 {
        self.world.town.params.population
    }
}

/// How far a hero's level has grown its travel powers: 0 at level 1, 1 at
/// level 20 (the cap).
pub fn travel_growth(level: u8) -> f64 {
    (level.max(1) - 1) as f64 / 19.0
}

fn grown(level: u8, at_1: f64, at_20: f64) -> f64 {
    at_1 + (at_20 - at_1) * travel_growth(level).min(1.0)
}

/// How high a full super jump (jump held) goes, in metres: over a three
/// storey house at level 1, over a ten storey block at level 20. A tapped
/// jump goes about half as high (see [`SUPER_JUMP_CUT`]).
pub fn super_jump_height(level: u8) -> f64 {
    grown(level, 14.0, 45.0)
}

/// Take-off speed of a super jump that reaches [`super_jump_height`].
pub fn super_jump_speed(level: u8) -> f64 {
    (2.0 * GRAVITY * super_jump_height(level)).sqrt()
}

/// Top speed through the air while super jumping (m/s).
pub fn super_leap_speed(level: u8) -> f64 {
    grown(level, 15.0, 22.0)
}

/// How hard a super jumper steers in the air (m/s²): enough to turn a leap
/// round in about half a second.
pub const SUPER_AIR_ACCEL: f64 = 45.0;
/// Letting go of jump on the way up makes gravity this many times stronger
/// until the top, so a tap is a short hop (indoors, onto a low roof) and a
/// held jump goes all the way.
pub const SUPER_JUMP_CUT: f64 = 2.0;
/// A super jumper landing faster than this (m/s down, about an eight metre
/// fall) lands hard: dust, a crack in the ground, a stomp that knocks
/// villains round them over.
pub const HARD_LANDING: f64 = 18.0;

/// Sprint's top speed (m/s): twice a run at level 1.
pub fn sprint_speed(level: u8) -> f64 {
    grown(level, 13.0, 17.0)
}

/// Running speed with Super Speed, with Shift held, and how quickly a
/// speedster gets going and stops.
pub fn super_speed(level: u8) -> f64 {
    grown(level, 24.0, 34.0)
}
pub fn super_speed_boost(level: u8) -> f64 {
    grown(level, 40.0, 58.0)
}
pub const SUPER_SPEED_ACCEL: f64 = 120.0;

fn horizontal_speed(body: &Body) -> f64 {
    DVec2::new(body.vel.x, body.vel.z).length()
}

/// What a hero's movement powers allow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Moves {
    /// Flight is unlocked.
    pub fly: bool,
    /// Super Jump is owned.
    pub super_jump: bool,
    /// Super Speed is owned.
    pub speed: bool,
    /// Sprint (a faster run), Glide (hold jump to fall slowly) and Wall
    /// Run (run up walls) are owned.
    pub sprint: bool,
    pub glide: bool,
    pub wall_run: bool,
    /// The hero's level: travel powers grow with it.
    pub level: u8,
}

impl Moves {
    /// A hero without movement powers (and villains).
    pub const ON_FOOT: Moves = Moves {
        fly: false,
        super_jump: false,
        speed: false,
        sprint: false,
        glide: false,
        wall_run: false,
        level: 1,
    };
    /// Flight allowed (tests and tools that fly).
    pub const FLYER: Moves = Moves {
        fly: true,
        ..Moves::ON_FOOT
    };

    /// From what a hero's [`hw_net::CombatState`] carries: its effect bits
    /// and its level.
    pub fn from_view(c: &hw_net::CombatState) -> Self {
        let effects = c.effects;
        Self {
            level: c.level,
            fly: effects & hw_net::EFFECT_CAN_FLY != 0,
            super_jump: effects & hw_net::EFFECT_SUPER_JUMP != 0,
            speed: effects & hw_net::EFFECT_SUPER_SPEED != 0,
            sprint: effects & hw_net::EFFECT_SPRINT != 0,
            glide: effects & hw_net::EFFECT_GLIDE != 0,
            wall_run: effects & hw_net::EFFECT_WALL_RUN != 0,
        }
    }

    /// What a hero's powers allow at `level`.
    pub fn of(kit: &powers::Kit, level: u8) -> Self {
        use powers::travel;
        Self {
            level,
            fly: kit.travels(travel::FLY),
            super_jump: kit.travels(travel::JUMP),
            speed: kit.travels(travel::SPEED),
            sprint: kit.travels(travel::SPRINT),
            glide: kit.travels(travel::GLIDE),
            wall_run: kit.travels(travel::WALL),
        }
    }
}

/// A glider falls no faster than this (m/s).
pub const GLIDE_FALL: f64 = 2.5;
/// A wall runner climbs this fast (m/s).
pub const WALL_CLIMB: f64 = 9.0;

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
        // A flier is not held to the ground (see `move_body`).
        body.grounded = false;
        solids.move_body(body, DT);
        if body.pos.y > FLY_CEILING {
            body.pos.y = FLY_CEILING;
            body.vel.y = body.vel.y.min(0.0);
        }
        return;
    }
    let level = moves.level;
    let top = if moves.speed && body.grounded {
        if input.boost {
            super_speed_boost(level)
        } else {
            super_speed(level)
        }
    } else if moves.super_jump && !body.grounded {
        // A leap keeps a speedster's run if that is faster.
        super_leap_speed(level).max(if moves.speed {
            horizontal_speed(body)
        } else {
            0.0
        })
    } else if moves.speed {
        // Off the ground a speedster keeps most of their run.
        super_speed(level).max(horizontal_speed(body))
    } else if moves.sprint {
        sprint_speed(level)
    } else {
        RUN_SPEED
    };
    let want = DVec3::new(input.move_x as f64, 0.0, input.move_z as f64) * top;
    let horizontal = DVec3::new(body.vel.x, 0.0, body.vel.z);
    let delta = want - horizontal;
    let accel = if body.grounded && moves.speed {
        SUPER_SPEED_ACCEL
    } else if body.grounded {
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
    let rising_leap = moves.super_jump && !body.grounded && body.vel.y > JUMP_SPEED;
    if input.jump && body.grounded {
        if moves.super_jump {
            body.vel.y = super_jump_speed(level);
            // Launch: the leap carries on the way the hero was going at
            // once, instead of speeding up in the air.
            let stick = DVec2::new(input.move_x as f64, input.move_z as f64);
            if stick.length_squared() > 1e-6 {
                let keep = horizontal_speed(body).max(super_leap_speed(level) * 0.8);
                let v = stick.normalize() * keep * stick.length().min(1.0);
                let v = DVec3::new(v.x, 0.0, v.y);
                body.vel.x = v.x;
                body.vel.z = v.z;
            }
        } else {
            body.vel.y = JUMP_SPEED;
        }
    }
    // Let go on the way up and the leap tops out sooner.
    let pull = if rising_leap && !input.jump {
        SUPER_JUMP_CUT
    } else {
        1.0
    };
    body.vel.y -= GRAVITY * pull * DT;
    // A glider holding jump floats down.
    if moves.glide && input.jump && !body.grounded {
        body.vel.y = body.vel.y.max(-GLIDE_FALL);
    }
    // A wall runner running into a wall runs up it.
    let way = DVec3::new(want.x, 0.0, want.z).normalize_or_zero();
    if moves.wall_run
        && way != DVec3::ZERO
        && solids.solid_at(body.pos + way * 0.7 + DVec3::Y * 1.0)
    {
        body.vel.y = body.vel.y.max(WALL_CLIMB);
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
/// How far down the street from the spawn the screenshot fight's group
/// stands (m); the hero is put [`FIRST_FIGHT_HERO`] along the same way.
pub const FIRST_FIGHT_AT: f64 = 32.0;
pub const FIRST_FIGHT_HERO: f64 = 19.0;

/// The way from the spawn `(mid, dir)` down which the screenshot fight
/// happens: back along the spawn line if the street runs on there, else
/// the first of the other ways that is open, so hero and villains stand in
/// the street and not inside a house.
pub fn first_fight_way(solids: &Obstacles, (mid, dir): (DVec2, DVec2)) -> (DVec2, DVec2) {
    let open = |way: DVec2| {
        let y = solids.ground_at(mid).max(GROUND_Y);
        let a = DVec3::new(mid.x, y + 1.2, mid.y);
        let far = mid + way * (FIRST_FIGHT_AT + 4.0);
        let b = DVec3::new(far.x, solids.ground_at(far).max(GROUND_Y) + 1.2, far.y);
        combat::clear_line(solids, a, b)
    };
    let ways = [-dir, dir, dir.perp(), -dir.perp()];
    let way = ways.into_iter().find(|&w| open(w)).unwrap_or(-dir);
    (mid, way)
}

pub struct Sim {
    pub world: World,
    pub seed: Seed,
    /// Size of the city: sent to joining players with the seed.
    pub town_population: f64,
    pub tick: u64,
    spawn: (DVec2, DVec2),
    /// Where heroes below [`quarter::ROOKIE_LEVEL`] start: a street near
    /// the Chrome Jackets' corners (none in a town without a gang).
    rookie: Option<(DVec2, DVec2)>,
    /// The portal home a hero without one gets: Hero Hall's plaza.
    hall_home: Option<gates::Side>,
    /// Players who appeared at the start and whose hero file has not come
    /// in yet: it decides which start is theirs.
    fresh: BTreeSet<PlayerId>,
    schedule: Schedule,
    track: Schedule,
    /// Microseconds each part of [`Sim::step`] took since the last
    /// [`Sim::take_step_times`] (the hitch log names what makes the server slow).
    step_times: Vec<(&'static str, u64)>,
    /// Parts of a step that overran their budget (see [`Sim::note_part`]).
    overruns: Vec<String>,
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
    /// The Aether Tube's capsules (see [`tube`]).
    pub tubes: tube::Tubes,
    /// The moving traffic and the cars wrecked in it (see [`wrecks`]).
    pub wrecks: wrecks::Wrecks,
    /// Trouble that comes to the heroes (see [`incidents`]).
    pub incidents: incidents::Incidents,
    /// The gangs' street crime near the heroes, and heroes stepping in
    /// (see [`crimes`]).
    pub crimes: crimes::Crimes,
    /// The Doctor's story: the chain of chapters (see [`story`]).
    pub story: story::Story,
    /// Indoor cases from Hero Hall (see [`missions`]).
    pub missions: missions::Missions,
    /// Trouble bystanders notice: they run, stare, cars stop (see
    /// [`alarms`]).
    pub alarms: alarms::Alarms,
    /// The alarm each open crime keeps up, by incident.
    crime_alarms: BTreeMap<u32, u32>,
    /// The factions' places in the deep underground near heroes.
    pub deep: deepspots::DeepSpots,
    /// The square of the town with people, traffic, taxis and incidents now
    /// (it follows the heroes, see [`Sim::follow_heroes`]).
    pub area: Option<Arc<Area>>,
    /// The next square, being made in the background.
    area_job: Option<std::sync::mpsc::Receiver<(Arc<Area>, traffic::Traffic)>>,
    /// The next square, made, waiting for the taxis to land.
    area_ready: Option<(Arc<Area>, traffic::Traffic)>,
    /// Has the square moved from the one every machine starts with?
    area_moved: bool,
    /// The rocket port's launches (see [`rocket`]).
    pub rockets: Vec<rocket::Pad>,
    /// The lifts of the game-scale interiors (they run by the clock).
    pub lifts: lifts::Lifts,
    /// The airships heroes bump into: the liner on its timetable and the
    /// pirates' moored ship.
    pub airships: airships::Airships,
    /// Doors people walked through, and news of doors opening and shutting.
    pub doors: doors::Doors,
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
        sim.deep = deepspots::DeepSpots::new(town.world.clone());
        sim.airships = airships::Airships::with_raids(&town.world);
        sim.street = props::PropDamage::new(town.world.clone()).with_solids(town.obstacles.clone());
        sim.area = Some(town.area.clone());
        {
            // The medical drone flies beaten heroes to the nearest of these.
            let mut f = sim.world.resource_mut::<Fights>();
            f.area = Some(combat::Skyline(town.area.clone()));
            f.wards = town
                .world
                .landmarks
                .iter()
                .filter_map(|l| l.ward())
                .collect();
        }
        sim.taxis = taxi::Taxis::new(town.area.clone());
        sim.tubes = tube::Tubes::new(town.world.clone());
        sim.rockets = rocket::pads(&town.world);
        sim.lifts = lifts::Lifts::new(&town.world);
        sim.wrecks = wrecks::Wrecks::new(traffic::Traffic::new(town.area.clone()));
        sim.incidents = incidents::Incidents::new(town.area.clone());
        sim.crimes.set_area(town.area.clone());
        let floors = Arc::new(floornav::Floors::with_hideouts(&town.world, &town.area));
        sim.world.resource_mut::<Fights>().floors = Some(floors.clone());
        sim.missions = missions::Missions::plan(&town.world, town.area.clone(), floors, seed.0);
        sim.world.resource_mut::<Fights>().streets = Some(Arc::new(streets::Streets::new(
            &town.world,
            streets::TANK_STREET,
        )));
        let solids = sim.solids();
        // The villain quarter's corners come from a square round the
        // quarter itself (the heroes' square may not hold all of it).
        // Its pieces may reach a little past its radius, and the hangouts
        // stand in the band round it; a huge quarter is capped.
        let villains = match &town.world.town.dystopia {
            Some(d) => {
                let half = (d.radius * 1.3 + 200.0).min(4000.0);
                let (lo, hi) = (d.centre - DVec2::splat(half), d.centre + DVec2::splat(half));
                if town.area.contains(lo) && town.area.contains(hi) {
                    town.area.clone()
                } else {
                    Arc::new(Area::around(town.world.clone(), d.centre, half))
                }
            }
            None => town.area.clone(),
        };
        let corners = quarter::corners(&villains);
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
        for l in quarter::lookouts(&villains, &corners) {
            if let Some(p) = quarter::perch(&solids, &l) {
                sim.add_lookout(p, l.facing);
            }
        }
        // The Chrome Jackets' corners in the band round the quarter, never
        // where heroes appear.
        let hangouts = quarter::hangouts(&villains, sim.spawn.0, HANGOUT_CLEAR);
        for c in &hangouts {
            let y = solids.ground_at(c.at).max(GROUND_Y);
            sim.add_hangout(DVec3::new(c.at.x, y, c.at.y), c.facing);
        }
        // The sky pirates' landing on the quay under their airship, never
        // where heroes appear; Molehaven's guards inside its gate.
        let roost = roosts::pirate_roost(&town.world).map(|r| r.watch());
        let mut roost_at = Vec::new();
        if let Some((at, facing)) = roost {
            if DVec2::new(at.x, at.z).distance(sim.spawn.0) > ROOST_CLEAR {
                sim.add_pirate_roost(at, facing);
                roost_at.push(DVec2::new(at.x, at.z));
            }
        }
        // New heroes start a short walk from the Jackets, where the small
        // crime is (see [`incidents::Incidents::set_hot`]).
        sim.rookie =
            quarter::rookie_start(&villains, &hangouts, sim.spawn.0, &roost_at, ROOST_CLEAR);
        sim.set_spawn(sim.spawn);
        sim.incidents
            .set_hot(hangouts.iter().map(|c| c.at).collect());
        sim.story = story::Story::plan(&town.world, &villains, &solids);
        if let Some((at, facing)) = roosts::mole_gate(town.world.underground()) {
            sim.add_mole_gate(at, facing);
        }
        // The town's own heroes on Hero Hall's plaza.
        if let Some(hall) = town
            .world
            .landmarks
            .iter()
            .find(|l| l.kind == hw_gen::urban::landmarks::LandmarkKind::HeroHall)
        {
            let spots: Vec<(DVec3, DVec2)> = hall
                .hero_spots()
                .into_iter()
                .map(|(p, f)| (DVec3::new(p.x, hall.base, p.y), f))
                .collect();
            if let Some((p, dir)) = hall.portal_home() {
                sim.hall_home = Some(gates::Side {
                    at: DVec3::new(p.x, hall.base, p.y),
                    yaw: hw_core::det::atan2(dir.x, dir.y),
                });
            }
            let tick = sim.tick;
            if let Some(&(p, _)) = spots.first() {
                let mut f = sim.world.resource_mut::<combat::Fights>();
                f.home = Some(p);
                f.home_is_hall = true;
            }
            npcheroes::spawn(&mut sim.world, tick, &spots);
        }
        sim
    }

    /// Moves the square with people, traffic, taxis and incidents along
    /// with the heroes: once they are [`AREA_SHIFT`] from its centre the
    /// next one is made in the background and swapped in when ready (and
    /// no taxi is out). Clients hear of it ([`ToClient::AreaMoved`]).
    pub fn follow_heroes(&mut self, heroes: &[DVec3]) {
        let Some(area) = self.area.clone() else {
            return;
        };
        if let Some(rx) = &self.area_job {
            match rx.try_recv() {
                Ok(next) => {
                    self.area_job = None;
                    self.area_ready = Some(next);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.area_job = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.area_ready.is_some() && !self.taxis.busy() {
            if let Some((next, traffic)) = self.area_ready.take() {
                self.set_area(next, traffic);
            }
            return;
        }
        if heroes.is_empty() || self.area_job.is_some() || self.area_ready.is_some() {
            return;
        }
        let c = heroes
            .iter()
            .map(|p| DVec2::new(p.x, p.z))
            .fold(DVec2::ZERO, |a, b| a + b)
            / heroes.len() as f64;
        let now = (area.lo + area.hi) * 0.5;
        if (c - now).abs().max_element() < AREA_SHIFT {
            return;
        }
        let centre = (c / AREA_SNAP).round() * AREA_SNAP;
        let world = area.world.clone();
        let half = (area.hi.x - area.lo.x) * 0.5;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let next = Arc::new(Area::around(world, centre, half));
            let traffic = traffic::Traffic::new(next.clone());
            let _ = tx.send((next, traffic));
        });
        self.area_job = Some(rx);
    }

    /// Swaps in another square of the town now.
    pub fn set_area(&mut self, area: Arc<Area>, mut traffic: traffic::Traffic) {
        traffic.reset_at(self.tick as f64 * DT);
        let centre = (area.lo + area.hi) * 0.5;
        self.wrecks.set_traffic(traffic, centre);
        self.taxis.set_town(area.clone());
        self.incidents.set_town(area.clone());
        self.world.resource_mut::<combat::Fights>().area = Some(combat::Skyline(area.clone()));
        self.crimes.set_area(area.clone());
        self.missions.set_area(area.clone());
        self.area = Some(area);
        self.area_moved = true;
    }

    /// For a late joiner: where the square is, if it is not where it
    /// started (sent before the traffic in it).
    pub fn area_news(&self) -> Option<ToClient> {
        let a = self.area.as_ref().filter(|_| self.area_moved)?;
        let c = (a.lo + a.hi) * 0.5;
        Some(ToClient::AreaMoved { x: c.x, z: c.y })
    }

    /// [`Self::follow_heroes`], waiting for the next square if one is
    /// made (tests).
    pub fn finish_area_move(&mut self, heroes: &[DVec3]) {
        self.follow_heroes(heroes);
        if let Some(rx) = self.area_job.take() {
            if let Ok(next) = rx.recv() {
                self.area_ready = Some(next);
            }
        }
        self.follow_heroes(heroes);
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

    /// The sky pirates' lookouts on the quay: buccaneers and a gunner, at
    /// the team's level, while a hero is near; they warn strangers off.
    /// Returns the spot's index.
    pub fn add_pirate_roost(&mut self, at: DVec3, facing: DVec2) -> usize {
        let i = self.add_villain_corner(at, facing, 0, quarter::WAKE);
        let mut f = self.world.resource_mut::<Fights>();
        let spot = &mut f.spots[i];
        spot.crew = Some((combat::EnemyKind::Buccaneer, combat::EnemyKind::Gunner));
        spot.size = Some(roosts::ROOST_GROUP);
        i
    }

    /// Molehaven's gate guards: delvers inside the cavern. They let
    /// strangers pass (Molehaven is neutral) but warn off a hero who has
    /// beaten mole folk. Returns the spot's index.
    pub fn add_mole_gate(&mut self, at: DVec3, facing: DVec2) -> usize {
        let i = self.add_villain_corner(at, facing, 0, MOLE_WAKE);
        let mut f = self.world.resource_mut::<Fights>();
        let spot = &mut f.spots[i];
        spot.only = Some(combat::EnemyKind::Delver);
        spot.size = Some(roosts::GATE_GROUP);
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
        world.insert_resource(SpawnLine(DVec2::ZERO, DVec2::X, None));
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
            rookie: None,
            hall_home: None,
            fresh: Default::default(),
            schedule,
            track,
            step_times: Vec::new(),
            overruns: Vec::new(),
            physics: Physics::new(solids),
            bubbles: Bubbles::default(),
            damage: Damage::default(),
            street: props::PropDamage::default(),
            taxis: taxi::Taxis::default(),
            tubes: tube::Tubes::default(),
            wrecks: wrecks::Wrecks::default(),
            incidents: incidents::Incidents::default(),
            crimes: crimes::Crimes::default(),
            story: story::Story::default(),
            missions: missions::Missions::default(),
            alarms: alarms::Alarms::default(),
            crime_alarms: BTreeMap::new(),
            deep: deepspots::DeepSpots::default(),
            area: None,
            area_job: None,
            area_ready: None,
            area_moved: false,
            rockets: Vec::new(),
            lifts: lifts::Lifts::default(),
            airships: airships::Airships::default(),
            doors: doors::Doors::default(),
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
                    IncidentKind::CarCrash => AlarmKind::Crash,
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
        self.world
            .insert_resource(SpawnLine(line.0, line.1, self.rookie));
        // Hero's Return goes here until there is a Hero Hall.
        let home = self.point_on(line, 0);
        let mut f = self.world.resource_mut::<combat::Fights>();
        if f.home.is_none() || !f.home_is_hall {
            f.home = Some(home);
        }
    }

    pub fn solids(&self) -> Arc<Obstacles> {
        self.world.resource::<Solids>().0.clone()
    }

    /// Where the `k`-th player appears: in a row along the central street.
    pub fn spawn_point(&self, k: u32) -> DVec3 {
        self.point_on(self.spawn, k)
    }

    /// Where the `k`-th new hero (below [`quarter::ROOKIE_LEVEL`]) starts:
    /// in a row along a street near the Jackets' corners, or the central
    /// street in a town without them.
    pub fn rookie_point(&self, k: u32) -> DVec3 {
        self.point_on(self.rookie.unwrap_or(self.spawn), k)
    }

    /// The street new heroes start on, if the town has one apart from its
    /// centre.
    pub fn rookie_line(&self) -> Option<(DVec2, DVec2)> {
        self.rookie
    }

    /// Spawns `owner`'s hero at the start; once their hero file comes in
    /// ([`Self::set_sheet`]), a hero below [`quarter::ROOKIE_LEVEL`] moves
    /// to the rookies' street, unless a stronger friend is playing.
    pub fn spawn_at_start(&mut self, owner: PlayerId) -> NetId {
        let id = self.spawn_hero(owner, self.spawn_point(owner.0 as u32));
        self.fresh.insert(owner);
        id
    }

    /// A fresh hero's first file is in: the rookies go to their street.
    fn place_fresh(&mut self, owner: PlayerId) {
        if !self.fresh.remove(&owner) || self.rookie.is_none() {
            return;
        }
        let mut mine = None;
        let mut strongest = 1;
        for (hero, p, net) in self
            .world
            .query::<(&Hero, &Progress, &Net)>()
            .iter(&self.world)
        {
            let level = progress::level_of(p.sheet.lifetime);
            if hero.owner == owner {
                mine = Some((net.0, level));
            } else {
                strongest = strongest.max(level);
            }
        }
        let Some((id, level)) = mine else {
            return;
        };
        let k = owner.0 as u32;
        let here = self.position(id).unwrap_or_default();
        // Still where they appeared (not moved by a view or the debug menu).
        let unmoved = here.distance(self.spawn_point(k)) < 5.0;
        if unmoved && level.max(strongest) < quarter::ROOKIE_LEVEL {
            self.set_body(
                id,
                Body {
                    pos: self.rookie_point(k),
                    ..Default::default()
                },
            );
        }
    }

    fn point_on(&self, line: (DVec2, DVec2), k: u32) -> DVec3 {
        let (mid, dir) = line;
        let offset = (k as f64 - 1.0) * 2.5 * if k.is_multiple_of(2) { 1.0 } else { -1.0 };
        let p = mid + dir * offset;
        let ground = self.world.resource::<Solids>().0.ground_at(p).max(GROUND_Y);
        DVec3::new(p.x, ground + 0.5, p.y)
    }

    /// Advance the simulation by one fixed tick: heroes move, bubbles
    /// regroup, physics steps inside the bubbles, changes are stamped.
    /// Adds the time since `t` to the part `name` of the step, and starts the
    /// next part.
    fn lap(&mut self, name: &'static str, t: &mut std::time::Instant) {
        let us = t.elapsed().as_micros() as u64;
        *t = std::time::Instant::now();
        self.note_part(name, us);
    }

    /// Books `us` microseconds of one tick on the part `name`; a part that
    /// takes more than [`PART_BUDGET_US`] in one tick is an overrun, and is
    /// written down with how much there was to do (the hitch log prints it).
    fn note_part(&mut self, name: &'static str, us: u64) {
        match self.step_times.iter_mut().find(|(k, _)| *k == name) {
            Some(e) => e.1 += us,
            None => self.step_times.push((name, us)),
        }
        if us > PART_BUDGET_US && self.overruns.len() < 8 {
            let villains = self
                .world
                .query::<&combat::Enemy>()
                .iter(&self.world)
                .count();
            let f = self.world.resource::<combat::Fights>();
            let (searches, [full, reduced, dormant]) = (f.path_searches, f.tier_counts);
            self.overruns.push(format!(
                "server tick {}: '{name}' took {:.1} ms (budget {:.0} ms) with {villains} villains ({full} full, {reduced} reduced, {dormant} dormant), {searches} path searches",
                self.tick,
                us as f64 / 1000.0,
                PART_BUDGET_US as f64 / 1000.0,
            ));
        }
    }

    /// Overruns of the step's parts since the last call (see [`Self::note_part`]).
    pub fn take_overruns(&mut self) -> Vec<String> {
        std::mem::take(&mut self.overruns)
    }

    /// The step's parts and their microseconds since the last call.
    pub fn take_step_times(&mut self) -> Vec<(&'static str, u64)> {
        std::mem::take(&mut self.step_times)
    }

    pub fn step(&mut self) {
        let t0 = std::time::Instant::now();
        let mut t = t0;
        self.schedule.run(&mut self.world);
        self.lap("srv schedule us", &mut t);
        combat::step(&mut self.world, self.tick);
        for (k, us) in std::mem::take(&mut self.world.resource_mut::<combat::Fights>().laps) {
            self.note_part(k, us);
        }
        self.lap("srv combat us", &mut t);
        sidekick::step(&mut self.world, self.tick);
        self.lap("srv sidekick us", &mut t);
        npcheroes::step(&mut self.world, self.tick, &self.incidents.open);
        self.lap("srv npcheroes us", &mut t);
        self.machines_break_the_city();
        self.incidents
            .collateral
            .clone_from(&self.damage.collateral);
        self.incidents.step(&mut self.world, self.tick);
        if self.tick.is_multiple_of(TICK_HZ as u64) {
            self.incidents
                .vandalise(&mut self.world, self.tick, &mut self.street);
        }
        self.crimes.step(
            &mut self.world,
            self.tick,
            &mut self.street,
            &mut self.alarms,
        );
        self.lap("srv incidents+crimes us", &mut t);
        self.story.step(&mut self.world, self.tick);
        self.missions.step(&mut self.world, self.tick);
        if self.tick.is_multiple_of(TICK_HZ as u64) {
            self.incidents
                .set_emboldened(self.missions.emboldened.iter().map(|e| e.0).collect());
        }
        self.raise_crime_alarms();
        self.lap("srv story+missions us", &mut t);
        let solids = self.solids();
        taxi::step(&mut self.world, &mut self.taxis, &solids, self.tick, DT);
        tube::step(&mut self.world, &mut self.tubes, self.tick, DT);
        self.alarms.step(self.tick);
        self.lap("srv taxi+tube+alarms us", &mut t);
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
        // Heroes bump into the cars near them (and can stand on them),
        // into the rocket and its arms, and into the airships, each where
        // the game clock has it this tick.
        let heroes: Vec<DVec3> = self
            .world
            .query::<(&Motion, &Hero)>()
            .iter(&self.world)
            .map(|(m, _)| m.0.pos)
            .collect();
        // A crowd stops a car as one body does: one stand-in per 2 m square,
        // at most MAX_BLOCKERS of them (nearest the heroes first), so a
        // thousand villains cost the traffic no more than a few hundred.
        let standing = thin_out(standing, &heroes);
        let now = self.tick as f64 * DT;
        let raids: Vec<(u32, DVec3)> = self
            .incidents
            .open
            .iter()
            .filter(|i| i.kind == incidents::IncidentKind::PirateRaid)
            .map(|i| (i.id, i.at))
            .collect();
        self.airships.set_raids(&raids);
        let saucers: Vec<(u32, DVec3)> = self
            .incidents
            .open
            .iter()
            .filter(|i| i.kind == incidents::IncidentKind::SaucerRaid)
            .map(|i| (i.id, i.at))
            .collect();
        self.airships.set_saucers(&saucers);
        let mut movers = rocket::solids(&self.rockets, now, &heroes, 25.0);
        movers.extend(self.airships.solids_near(&heroes, now));
        movers.extend(self.story.statue_solids());
        movers.extend(self.missions.safe_solids());
        movers.extend(self.lifts.solids_near(&heroes, 25.0, now));
        self.lap("srv movers us", &mut t);
        if let Some(t) = self.wrecks.traffic.as_mut() {
            t.set_blockers(&standing, &stops);
            t.set_blocked(&self.alarms.road_blocks());
            t.advance_to(now);
            movers.extend(t.car_solids(&heroes, 25.0, now));
        }
        self.world.resource::<Solids>().0.set_movers(movers);
        self.lap("srv traffic us", &mut t);
        self.walk_through_doors();
        self.keep_gate_homes();
        self.pay_karma();
        self.sync_levels();
        self.clear_rubble();
        self.lap("srv doors+karma+levels+rubble us", &mut t);
        let heroes: Vec<(PlayerId, NetId, DVec3)> = self
            .world
            .query::<(&Hero, &Net, &Motion)>()
            .iter(&self.world)
            .map(|(h, n, m)| (h.owner, n.0, m.0.pos))
            .collect();
        let players: Vec<(PlayerId, DVec3)> = heroes.iter().map(|(p, _, at)| (*p, *at)).collect();
        self.bubbles.update(&players);
        self.strike();
        self.lap("srv bubbles+strike us", &mut t);
        if self.tick.is_multiple_of(TICK_HZ as u64) {
            let solids = self.solids();
            let at: Vec<DVec3> = heroes.iter().map(|h| h.2).collect();
            self.damage.repair_unseen(&solids, &at, self.tick, TICK_HZ);
            self.street.repair_unseen(&at, self.tick, TICK_HZ);
            self.wrecks.repair_unseen(&at, self.tick, TICK_HZ);
            // The town plans the ground ahead of the heroes in the
            // background, so walking or flying on finds it ready.
            solids.prefetch(&at, PREFETCH_REACH);
            {
                // Mole camps, lab guards and relic hunters near heroes below.
                let mut fights = self.world.resource_mut::<Fights>();
                self.deep.update(&mut fights, &at);
            }
            self.follow_heroes(&at);
        }
        self.lap("srv every-second us", &mut t);
        self.step_physics(&heroes);
        self.lap("srv physics us", &mut t);
        self.tick += 1;
        self.world.resource_mut::<Now>().0 = self.tick;
        self.track.run(&mut self.world);
        self.lap("srv track us", &mut t);
    }

    /// The colossus's feet, hull and shells break what they hit.
    fn machines_break_the_city(&mut self) {
        let blows = std::mem::take(&mut self.world.resource_mut::<Fights>().city);
        if blows.is_empty() {
            return;
        }
        for (at, radius, power) in blows {
            let (_, _) = self.street.hit(at, DVec3::Y, radius, power, self.tick);
            let (_, _) = self.wrecks.hit(at, DVec3::Y, radius, power, self.tick);
            let hit = self.hit_buildings(at, radius, power, None);
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
            // Blasting needs a bolt power.
            let blast = input.blast && fighter.is_none_or(|f| f.kit.blast.is_some());
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
                let hit = self.hit_buildings(at, radius, strength, Some(owner));
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

    /// A blow at the buildings round `at` (`by`: the hero who struck, for
    /// the collateral tally). A building it brings down traps people.
    pub fn hit_buildings(
        &mut self,
        at: DVec3,
        radius: f32,
        power: f32,
        by: Option<PlayerId>,
    ) -> destruction::HitResult {
        let solids = self.solids();
        let hit = self.damage.hit(&solids, at, radius, power, by, self.tick);
        for &b in &hit.collapsed {
            self.incidents.collapsed(b);
        }
        hit
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
        news.extend(self.crimes.take_news());
        news.extend(self.story.take_news());
        news.extend(self.missions.take_news());
        news.extend(self.damage.take_news());
        news.extend(self.doors.take_news());
        news.extend(self.world.resource_mut::<Fights>().gates.take_news());
        news
    }

    /// Which world a portal home was marked in: the seed and the town's
    /// size (the same seed at another size is another town).
    pub fn world_key(&self) -> u64 {
        self.seed.0 ^ self.town_population.to_bits().rotate_left(17)
    }

    /// Whether the server knows `owner`'s portal home (a test helper).
    pub fn gates_home_known(&self, owner: PlayerId) -> bool {
        self.world.resource::<Fights>().gates.home(owner).is_some()
    }

    /// The open portals, for a joining player.
    pub fn gates_snapshot(&self) -> ToClient {
        self.world.resource::<Fights>().gates.snapshot()
    }

    /// Portal homes marked this tick go into their heroes' files (so they
    /// are kept on the player's PC); a home from a joining hero's file is
    /// used if it was marked in this world on open ground. Pairs of
    /// players who left close.
    fn keep_gate_homes(&mut self) {
        let marks = self.world.resource_mut::<Fights>().gates.take_marks();
        let key = self.world_key();
        for (owner, s) in marks {
            let home = hw_net::GateHome::new(key, s.at.to_array(), s.yaw);
            self.change_sheet(owner, |sheet| sheet.gate_home = Some(home));
        }
        if !self.tick.is_multiple_of(TICK_HZ as u64) {
            return;
        }
        let here: Vec<(PlayerId, Option<hw_net::GateHome>)> = self
            .world
            .query::<(&Hero, &Progress)>()
            .iter(&self.world)
            .map(|(h, p)| (h.owner, p.sheet.gate_home))
            .collect();
        let solids = self.solids();
        let mut fights = self.world.resource_mut::<Fights>();
        let owners: Vec<PlayerId> = here.iter().map(|h| h.0).collect();
        let _ = fights.gates.keep_only(&owners);
        for (owner, home) in here {
            let Some(h) = home.filter(|h| h.world == key) else {
                continue;
            };
            let s = gates::Side {
                at: DVec3::from_array(h.pos()),
                yaw: h.yaw(),
            };
            let open = !solids.solid_at(s.at + DVec3::Y * 1.0);
            if open && fights.gates.home(owner).is_none() {
                fights.gates.remember_home(owner, s);
            }
        }
    }

    /// Player `player`'s hero opens (`open`) or shuts a door it stands at.
    pub fn use_door(&mut self, player: PlayerId, building: u64, key: u32, open: bool) {
        let Some(at) = self.hero_of(player).and_then(|h| self.position(h)) else {
            return;
        };
        let people = self.people_feet();
        let solids = self.solids();
        if let Err(why) = self
            .doors
            .hero_uses(&solids, at, building, key, open, &people)
        {
            println!("door {building}/{key}: {why:?}");
        }
    }

    /// Everyone's feet: heroes and everyone else with a body.
    fn people_feet(&mut self) -> Vec<DVec3> {
        self.world
            .query_filtered::<&Motion, Without<Prop>>()
            .iter(&self.world)
            .map(|m| m.0.pos)
            .collect()
    }

    /// People on foot open shut doors they walk up to and shut them behind
    /// them (heroes use E).
    fn walk_through_doors(&mut self) {
        if !self.tick.is_multiple_of(3) {
            return;
        }
        let mut walkers = Vec::new();
        let mut everyone = Vec::new();
        let mut heroes = Vec::new();
        for (m, hero) in self
            .world
            .query_filtered::<(&Motion, Has<Hero>), Without<Prop>>()
            .iter(&self.world)
        {
            everyone.push(m.0.pos);
            if hero {
                heroes.push(m.0.pos);
            } else if m.0.grounded {
                walkers.push((m.0.pos, m.0.vel));
            }
        }
        let solids = self.solids();
        self.doors.step(
            &solids,
            &walkers,
            &everyone,
            &heroes,
            self.tick,
            TICK_HZ as f64,
        );
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

    /// Like [`Self::add_fight_spot`], but `size` villains all of `kind`
    /// (tests of one kind's rules).
    pub fn add_fight_spot_of(
        &mut self,
        at: DVec3,
        facing: DVec2,
        kind: combat::EnemyKind,
        size: u32,
    ) -> usize {
        let i = {
            let mut f = self.world.resource_mut::<Fights>();
            f.spots.push(Spot {
                only: Some(kind),
                size: Some(size),
                ..Spot::fight(at, facing)
            });
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
        let solids = self.world.resource::<Solids>().0.clone();
        let (mid, way) = first_fight_way(&solids, self.spawn);
        let p = mid + way * FIRST_FIGHT_AT;
        let ground = solids.ground_at(p).max(GROUND_Y);
        self.add_fight_spot(DVec3::new(p.x, ground, p.y), -way)
    }

    /// The encounter spots and what happened in the last tick.
    /// Villains act at most every `stride`th tick (tests of how a slow turn
    /// rate affects what they do).
    pub fn set_think_stride(&mut self, stride: u64) {
        self.world.resource_mut::<Fights>().min_think_stride = stride.max(1);
    }

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
        let mut sheet = progress::sanitize(sheet);
        // A hero without a portal home in this town starts with it marked
        // on Hero Hall's plaza (they can mark a new one any time).
        let key = self.world_key();
        if let Some(h) = self.hall_home {
            if sheet.gate_home.is_none_or(|g| g.world != key) {
                sheet.gate_home = Some(hw_net::GateHome::new(key, h.at.to_array(), h.yaw));
            }
        }
        self.change_sheet(owner, |s| *s = sheet);
        self.place_fresh(owner);
    }

    /// `owner` buys `power`. Returns the price paid.
    pub fn buy(&mut self, owner: PlayerId, power: PowerId) -> Result<u32, progress::BuyError> {
        let mut result = Err(progress::BuyError::NotForSale);
        self.change_sheet(owner, |s| result = progress::buy(s, power));
        result
    }

    /// `owner` gives `power` up. Returns the karma refunded.
    pub fn give_up(
        &mut self,
        owner: PlayerId,
        power: PowerId,
    ) -> Result<u32, progress::GiveUpError> {
        let mut back = Err(progress::GiveUpError::NotOwned);
        self.change_sheet(owner, |s| back = progress::give_up(s, power));
        back
    }

    /// Sets `owner`'s element (the debug menu's switch: an element is
    /// otherwise for good).
    pub fn set_element(&mut self, owner: PlayerId, element: powers::Element) {
        self.change_sheet(owner, |s| s.element = element.code());
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
                // Toggles that are on stay on (those still owned).
                let on = fighter.kit.toggles_on();
                fighter.kit = Kit::of(&p.sheet);
                fighter.kit.set_toggles(on);
                // A power given up lets go of what it held.
                if !fighter.kit.has(fighter.lifted_with) {
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
        let friends = self.world.resource::<Fights>().friends.clone();
        for (hero, faction, by) in friends {
            for (n, mut p) in self
                .world
                .query::<(&Net, &mut Progress)>()
                .iter_mut(&mut self.world)
            {
                if n.0 == hero {
                    factions::befriend(&mut p.sheet.regard, faction, by);
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
                let up = progress::earn(&mut p.sheet, r.karma);
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
        self.fresh.remove(&owner);
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

    /// `owner` calls a capsule to station `dest` of the Aether Tube (see
    /// [`tube`]), or with [`tube::HOP_OUT`] gets out of it.
    pub fn ride_tube(&mut self, owner: PlayerId, dest: u16) {
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
        tube::call(
            &mut self.world,
            &mut self.tubes,
            id,
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
                (
                    Has<Prop>,
                    Option<&Enemy>,
                    Option<&Drone>,
                    Option<&CombatView>,
                ),
                (
                    &LastChange,
                    Option<&taxi::Fare>,
                    Option<&robots::Mount>,
                    Option<&incidents::Bystander>,
                    Option<&tube::Passenger>,
                    Option<&npcheroes::NpcHero>,
                    Option<&sidekick::Sidekick>,
                ),
            )>()
            .iter(&self.world)
            .map(
                |(
                    n,
                    m,
                    h,
                    o,
                    (prop, enemy, drone, view),
                    (c, fare, mount, bystander, rider, npc, sidekick),
                )| {
                    (
                        EntityState {
                            id: n.0,
                            kind: if prop {
                                EntityKind::Crate
                            } else if fare.is_some() {
                                EntityKind::Taxi
                            } else if rider.is_some() {
                                EntityKind::Capsule
                            } else if let Some(e) = enemy {
                                e.kind.entity_kind()
                            } else if drone.is_some() {
                                EntityKind::Drone
                            } else if bystander.is_some() {
                                EntityKind::Bystander
                            } else if npc.is_some() {
                                EntityKind::NpcHero
                            } else if sidekick.is_some() {
                                EntityKind::Sidekick
                            } else {
                                EntityKind::Hero
                            },
                            combat: view.map(|v| v.0),
                            pos: m.0.pos.to_array(),
                            vel: m.0.vel.as_vec3().to_array(),
                            rot: o.map_or(hw_net::protocol::NO_ROTATION, |o| o.0),
                            grounded: m.0.grounded,
                            owner: h
                                .map(|h| h.owner)
                                .or(fare.map(|f| f.0))
                                .or(rider.map(|r| r.0))
                                .or(sidekick.map(|s| s.player)),
                            // Looks that must be a particular one ride on
                            // the entity's own id (see `Enemy::look`).
                            mount: mount
                                .map(|m| (m.on, m.part))
                                .or(npc.map(|h| (n.0, h.index)))
                                .or(sidekick.map(|s| (s.owner, s.theme)))
                                // A drone: whom it fetches, and medical (1)
                                // or police (0).
                                .or(drone.map(|d| (d.carry, d.medical as u8)))
                                .or(enemy.and_then(|e| e.look).map(|l| (n.0, l)))
                                .or(bystander.and_then(|b| b.look).map(|l| (n.0, l))),
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
    mut fights: ResMut<Fights>,
    mut q: Query<(&Net, &Control, &mut Motion, Option<&mut Fighter>), WalkingHero>,
) {
    for (net, control, mut motion, mut fighter) in &mut q {
        let moves = fighter
            .as_ref()
            .map_or(Moves::ON_FOOT, |f| Moves::of(&f.kit, f.level));
        let status = fighter.as_ref().map_or(Status::Standing, |f| f.status);
        let input = match status {
            // A drone holds the carried; the downed only fall.
            Status::Carried => continue,
            Status::Down => InputState::default(),
            _ => control.0,
        };
        let mut body = motion.0;
        let before = body;
        step_moving(&mut body, input, &solids.0, moves);
        // Into a portal's front, out of its twin's (the client predicts the
        // same, see `predict`).
        let _ = fights.gates.carry(net.0, &before, &mut body);
        // A super jump's take-off and hard landing: the fight shows them
        // and the landing stomps (`combat::step` answers them this tick).
        if moves.super_jump {
            if let Some(f) = fighter.as_mut() {
                if before.grounded && !body.grounded && body.vel.y > JUMP_SPEED {
                    f.leapt = true;
                }
                if !before.grounded && body.grounded && before.vel.y < -HARD_LANDING {
                    f.landed = -before.vel.y as f32;
                }
            }
        }
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
        element: 0,
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
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn steps_count_ticks() {
        let mut sim = flat_sim(1);
        for _ in 0..3 {
            sim.step();
        }
        assert_eq!(sim.tick, 3);
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
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
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
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
        // Every hero has Super Jump, so one jump is a level-1 leap.
        let peak = super_jump_height(1);
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
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
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
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
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
