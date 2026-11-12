//! The mad scientist's machines: the robo-sentry's shield projector and the
//! walking tank, the colossus.
//!
//! A robot's breakable parts are fighters of their own, weak points
//! ([`EnemyKind::WeakPoint`] with a [`Mount`]): heroes Tab to them, see their
//! bars and blow them off. Each tick they ride where their part is on the
//! robot; the geometry here is shared with the client, which draws the same
//! parts (AssetGenerator models on Engine prefabs) in the same places.
//!
//! The robo-sentry (a lieutenant) shoots like a technician and shields every
//! villain near it while the dish on its back stands: break the dish first.
//!
//! The colossus is a set piece in three acts, and a rampage through town:
//! 1. It strides about, armoured (blows glance off), crushing what it walks
//!    into. A hero in sight, it leaves the streets and makes straight for
//!    them through the houses, rams buildings on its way down, shells
//!    heroes, stomps those under it, charges down a marked lane and rakes a
//!    marked line with its Aether lance. Every big blow is warned on the
//!    ground first. The four glowing knee actuators and the cannon are weak
//!    points; hit the cannon hard while the lance charges and it overloads,
//!    and the machine reels.
//! 2. With two knees gone it goes down on its belly: the clamshell opens, the
//!    Aether core shows and takes full damage, robo-sentries drop from the
//!    hatch, and the core discharges round it (fly or jump clear).
//! 3. After [`KNEEL_SECONDS`] it hauls itself up on two legs, enraged:
//!    shells come in salvos, charges and the lance come sooner. Break the
//!    other two knees and it goes down for good.
//!
//! Beaten, its wreck smokes a while and is hauled off.

use crate::collision::{Body, Obstacles, BODY_HEIGHT};
use crate::combat::{
    enemy_spec, event, hit, level_scale, spawn_enemy, ticks, Actor, Enemy, EnemyKind, Fights,
};
use crate::streets::Streets;
use crate::{Motion, Net, Orientation, Solids, DT, GROUND_Y};
use bevy_ecs::prelude::*;
use glam::{DVec2, DVec3};
use hw_core::NetId;
use hw_net::{Fx, FxKind, Status, EFFECT_ARMOURED, EFFECT_ENRAGED, EFFECT_KNEELING};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

/// Screenshots (`HW_COLOSSUS_RAMPAGE=1`): rampage as if a hero were always
/// in sight.
static ALWAYS_SEEN: AtomicBool = AtomicBool::new(false);

/// Makes every colossus rampage as if a hero were in sight (screenshots).
pub fn rampage_unseen(on: bool) {
    ALWAYS_SEEN.store(on, Ordering::Relaxed);
}

/// The colossus's core health (before the level and the team size).
pub const CORE_HEALTH: f32 = 1300.0;
/// Share of a blow that gets through shut armour.
pub const ARMOUR_LETS_THROUGH: f32 = 0.05;
/// Seconds it lies on its belly the first time before it gets up again.
pub const KNEEL_SECONDS: f64 = 25.0;
/// Seconds its wreck lies before it is hauled off.
pub const WRECK_SECONDS: f64 = 45.0;
/// Walking speed (m/s), and on two legs after its second wind.
pub const WALK_SPEED: f64 = 3.0;
const LIMP_SPEED: f64 = 2.4;
/// Turning (degrees a second).
const TURN: f64 = 30.0;
/// How near it comes to the hero it fights (m).
const KEEP: f64 = 12.0;
/// It goes for heroes this near (m), else walks on towards its goal.
const NOTICE: f64 = 220.0;
/// It leaves the streets and smashes buildings only while a hero is this
/// near (m): out of everyone's sight it keeps to the main roads.
pub const SEEN: f64 = 450.0;
/// Seconds between route plans along the streets.
const REPLAN: f64 = 3.0;
/// A street point this near (m) is reached.
const WAYPOINT: f64 = 3.0;
/// How far off (m) it wanders to when nobody fights it.
const WANDER: f64 = 600.0;
/// Seconds it heads for one building before it picks another.
const GOAL_SECONDS: f64 = 30.0;
/// Shells: the gap between them (s), calm and enraged, the warning before
/// one lands (s), its blast (m), the cannon's reach (m).
const SHELL_EVERY: (f64, f64) = (5.0, 4.0);
pub const SHELL_WINDUP: f64 = 1.8;
pub const SHELL_AREA: f64 = 7.0;
const SHELL_RANGE: (f64, f64) = (10.0, 170.0);
/// Enraged, it fires salvos: this many shells, this far apart across the
/// line of fire (m), landing this long after each other (s).
pub const SALVO: usize = 3;
const SALVO_SPREAD: f64 = 8.0;
const SALVO_GAP: f64 = 0.35;
/// The stomp: on heroes under it, every so often, after a warning.
const STOMP_EVERY: f64 = 5.0;
const STOMP_WINDUP: f64 = 1.2;
pub const STOMP_AREA: f64 = 12.0;
/// The charge: a warning (s), its speed (m/s), how near its middle (m) a
/// hero is run down, how often (s, calm and enraged), how far off the hero
/// must be (m), how far past the hero it runs (m) and how far at most.
pub const CHARGE_WINDUP: f64 = 1.8;
pub const CHARGE_SPEED: f64 = 9.0;
pub const CHARGE_WIDTH: f64 = 7.0;
const CHARGE_EVERY: (f64, f64) = (16.0, 11.0);
const CHARGE_RANGE: (f64, f64) = (24.0, 90.0);
const CHARGE_OVER: f64 = 15.0;
pub const CHARGE_MOST: f64 = 80.0;
/// The Aether lance: it plants itself and charges the cannon (s), then a
/// beam rakes a line on the ground from near to far (s). The line starts
/// this far out (m); heroes this near the beam (m) are burnt. How often
/// (s, calm and enraged), and the reach (m).
pub const LANCE_WINDUP: f64 = 2.6;
pub const LANCE_SECONDS: f64 = 1.2;
pub const LANCE_NEAR: f64 = 8.0;
pub const LANCE_WIDTH: f64 = 2.5;
const LANCE_EVERY: (f64, f64) = (15.0, 10.0);
const LANCE_RANGE: (f64, f64) = (20.0, 110.0);
const LANCE_OVER: f64 = 25.0;
pub const LANCE_MOST: f64 = 120.0;
/// Damage to the cannon while the lance charges (a share of its health)
/// that overloads it: the beam fizzles, the cannon loses this much more,
/// and the machine reels for a while.
pub const OVERLOAD_SHARE: f32 = 0.15;
const OVERLOAD_BURN: f32 = 0.3;
pub const STAGGER_SECONDS: f64 = 4.0;
/// Ramming a building in its way: the warning (s), the blow's reach round
/// where it lands (m), how often (s, nobody fighting it and in a fight),
/// and how far ahead (m) a building may be.
pub const SMASH_WINDUP: f64 = 1.3;
pub const SMASH_AREA: f64 = 6.0;
const SMASH_EVERY: (f64, f64) = (4.0, 9.0);
const SMASH_REACH: f64 = 18.0;
/// Its blows on heroes, as shares of its damage.
const STOMP_HITS: f32 = 1.5;
const PULSE_HITS: f32 = 1.2;
const CHARGE_HITS: f32 = 1.8;
const LANCE_HITS: f32 = 2.2;
const SMASH_HITS: f32 = 1.2;
/// The core's discharge while it is down.
const PULSE_EVERY: (f64, f64) = (7.0, 5.0);
const PULSE_WINDUP: f64 = 1.6;
pub const PULSE_AREA: f64 = 13.0;
/// Heroes this high over the ground are clear of a stomp or a discharge.
const CLEAR_HEIGHT: f64 = 2.5;
/// Robo-sentries that drop from its hatch when it goes down (per hero).
const DROPS: u8 = 2;
/// A robo-sentry's dish shields villains this near (m).
pub const PROJECTOR_REACH: f64 = 9.0;

/// Parts of the colossus: knees 0 to 3 (by [`leg_dir`]), the cannon 4.
pub const CANNON: u8 = 4;
pub const COLOSSUS_PARTS: [u8; 5] = [0, 1, 2, 3, CANNON];
/// The robo-sentry's dish.
pub const DISH: u8 = 0;

/// A weak point: the robot it is on and which part it is.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mount {
    pub on: NetId,
    pub part: u8,
}

/// A charge down a lane: it winds up until `go`, then runs from `from` to `to`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Charge {
    pub go: u64,
    pub from: DVec3,
    pub to: DVec3,
    /// Heroes already run down (bits by id).
    pub hit: u64,
}

/// The Aether lance: charging until `fire`, then raking the ground from
/// `near` to `far`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Lance {
    pub fire: u64,
    pub near: DVec3,
    pub far: DVec3,
    /// Heroes already burnt (bits by id).
    pub hit: u64,
    /// The cannon's health when it began charging (for an overload).
    pub cannon: f32,
    /// Metres of the line that have broken the city so far.
    pub done: f64,
}

/// How far the tank's middle keeps from a building of halls (m).
const HALLS_CLEAR: f64 = 3.0;
/// How long it keeps to the streets after such a building stood in its
/// way (s).
const ROUND_SECONDS: f64 = 20.0;

/// The walking tank's own state.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Colossus {
    /// Which way the hull faces (radians; 0 is +Z) and the turret aims.
    pub yaw: f64,
    pub aim: f64,
    /// Metres walked (the gait's phase) and footfalls so far.
    pub walked: f64,
    pub steps: u64,
    /// 0 standing, 1 down on its belly.
    pub kneel: f64,
    /// When it went down (while it is down).
    pub down_at: Option<u64>,
    /// Had its second wind.
    pub risen: bool,
    /// Shells on their way: (lands, where); more than one in a salvo.
    pub shells: [Option<(u64, DVec3)>; SALVO],
    pub next_shell: u64,
    pub stomp: Option<u64>,
    pub next_stomp: u64,
    pub pulse: Option<u64>,
    pub next_pulse: u64,
    pub charge: Option<Charge>,
    pub next_charge: u64,
    pub lance: Option<Lance>,
    pub next_lance: u64,
    /// A building rammed: (lands, where).
    pub smash: Option<(u64, DVec3)>,
    pub next_smash: u64,
    /// Reeling after an overload until this tick.
    pub stagger_until: u64,
    /// Where it marches when no hero is near, and when it chose it.
    pub goal: DVec3,
    pub goal_at: u64,
    /// Beaten: hauled off at this tick.
    pub wreck_at: Option<u64>,
    /// Robo-sentries to drop from the hatch.
    pub drop: u8,
    /// The team it was sized for.
    pub heroes: u8,
    /// Walking round a building of halls: which side (-1, 1; 0 not), and
    /// when its wall was last in the way.
    pub skirt: i8,
    pub skirt_at: u64,
}

impl Colossus {
    /// Winding up or in the middle of a big move (no walking, no new move).
    pub fn busy(&self, tick: u64) -> bool {
        self.planted(tick) || self.smash.is_some()
    }

    /// Standing its ground: charging, firing the lance or reeling (it rams
    /// buildings on the move).
    pub fn planted(&self, tick: u64) -> bool {
        self.charge.is_some() || self.lance.is_some() || tick < self.stagger_until
    }
}

// ---- Geometry (shared with the client) ----

/// The hull's underside over the ground, standing and down.
pub const HULL_STAND: f64 = 6.5;
pub const HULL_KNEEL: f64 = 2.4;
/// Hip sockets: how far out from the middle and how high on the hull.
pub const HIP_OUT: f64 = 3.2;
pub const HIP_UP: f64 = 0.9;
/// The thigh (hip to knee) and the shin (knee to sole), as built.
pub const THIGH: (f64, f64) = (4.2, 4.4);
pub const SHIN: (f64, f64) = (1.4, -10.9);
/// Where the feet rest, out from the middle, standing and sprawled.
pub const FOOT_OUT: f64 = 8.0;
pub const FOOT_SPRAWL: f64 = 9.4;
/// One stride (m): each foot moves this far per gait cycle.
pub const STRIDE: f64 = 4.8;
/// The order the feet step in (diagonals in turn).
pub const STEP_ORDER: [usize; 4] = [0, 2, 1, 3];
/// The turret ring on top of the hull, and the cannon's trunnion on it.
pub const TURRET_UP: f64 = 3.8;
pub const TRUNNION: (f64, f64) = (1.2, 1.9);
/// The core under the belly (below the hull's underside).
pub const CORE_DOWN: f64 = 1.3;

/// The hull's height over the ground for a kneel of `k` (0..1).
pub fn hull_height(k: f64) -> f64 {
    HULL_STAND + (HULL_KNEEL - HULL_STAND) * k.clamp(0.0, 1.0)
}

/// Direction of leg `k` from the hull's middle, in hull space (x, z): 30
/// degrees off the fore-and-aft line (front right, back right, back left,
/// front left), so the feet keep to a street about ten metres wide.
pub fn leg_dir(k: usize) -> DVec2 {
    let a = [30.0f64, 150.0, 210.0, 330.0][k % 4].to_radians();
    DVec2::new(a.sin(), a.cos())
}

/// Hull space to world: turn by `yaw` about +Y, then move to `at`.
pub fn to_world(at: DVec3, yaw: f64, local: DVec3) -> DVec3 {
    let (s, c) = (yaw.sin(), yaw.cos());
    at + DVec3::new(
        local.x * c + local.z * s,
        local.y,
        -local.x * s + local.z * c,
    )
}

/// Forward in the world for a yaw.
pub fn forward(yaw: f64) -> DVec3 {
    DVec3::new(yaw.sin(), 0.0, yaw.cos())
}

/// Leg `k`'s hip socket in hull space (over the ground).
pub fn hip(k: usize, kneel: f64) -> DVec3 {
    let d = leg_dir(k) * HIP_OUT;
    DVec3::new(d.x, hull_height(kneel) + HIP_UP, d.y)
}

/// Where leg `k`'s foot rests in hull space.
pub fn foot_rest(k: usize, kneel: f64) -> DVec3 {
    let out = FOOT_OUT + (FOOT_SPRAWL - FOOT_OUT) * kneel.clamp(0.0, 1.0);
    let d = leg_dir(k) * out;
    DVec3::new(d.x, 0.0, d.y)
}

/// The knee between a hip and a foot (two-bone IK), bending up and out.
pub fn knee(hip: DVec3, foot: DVec3) -> DVec3 {
    let t = THIGH.0.hypot(THIGH.1);
    let s = SHIN.0.hypot(SHIN.1);
    let d = foot - hip;
    let l = d.length().clamp((t - s).abs() + 0.05, t + s - 0.05);
    let dir = d.normalize_or(DVec3::NEG_Y);
    let a = (t * t - s * s + l * l) / (2.0 * l);
    let h = (t * t - a * a).max(0.0).sqrt();
    let mut n = DVec3::Y - dir * dir.dot(DVec3::Y);
    if n.length_squared() < 1e-6 {
        n = DVec3::new(hip.x, 0.0, hip.z);
    }
    hip + dir * a + n.normalize_or(DVec3::Y) * h
}

/// Where the cannon's weak point is in the world (half way down the barrel).
pub fn cannon_point(at: DVec3, c: &Colossus) -> DVec3 {
    let pivot = DVec3::new(
        at.x,
        at.y + hull_height(c.kneel) + TURRET_UP + TRUNNION.0,
        at.z,
    );
    pivot + forward(c.aim) * (TRUNNION.1 + 3.0)
}

/// The cannon's muzzle in the world (where shells and the lance leave).
pub fn muzzle_point(at: DVec3, c: &Colossus) -> DVec3 {
    let pivot = DVec3::new(
        at.x,
        at.y + hull_height(c.kneel) + TURRET_UP + TRUNNION.0,
        at.z,
    );
    pivot + forward(c.aim) * (TRUNNION.1 + 6.8)
}

/// Where the lance's beam meets the ground `s` (0..1) of the way along.
pub fn lance_point(l: &Lance, s: f64) -> DVec3 {
    l.near.lerp(l.far, s.clamp(0.0, 1.0))
}

/// Where weak point `part` of the colossus is in the world.
pub fn colossus_part(at: DVec3, c: &Colossus, part: u8) -> DVec3 {
    if part == CANNON {
        return cannon_point(at, c);
    }
    let k = part as usize % 4;
    to_world(at, c.yaw, knee(hip(k, c.kneel), foot_rest(k, c.kneel)))
}

/// The core in the world.
pub fn core_point(at: DVec3, c: &Colossus) -> DVec3 {
    DVec3::new(at.x, at.y + hull_height(c.kneel) - CORE_DOWN, at.z)
}

/// The robo-sentry's dish in the world (over its back).
pub fn dish_point(at: DVec3) -> DVec3 {
    at + DVec3::Y * 2.6
}

/// The rotation (x, y, z, w) of a yaw about +Y.
pub fn yaw_rotation(yaw: f64) -> [f32; 4] {
    let h = yaw * 0.5;
    [0.0, h.sin() as f32, 0.0, h.cos() as f32]
}

/// The yaw of a rotation made by [`yaw_rotation`].
pub fn rotation_yaw(rot: [f32; 4]) -> f64 {
    2.0 * (rot[1] as f64).atan2(rot[3] as f64)
}

fn turn_towards(from: f64, to: f64, max: f64) -> f64 {
    let mut d = (to - from) % std::f64::consts::TAU;
    if d > std::f64::consts::PI {
        d -= std::f64::consts::TAU;
    } else if d < -std::f64::consts::PI {
        d += std::f64::consts::TAU;
    }
    from + d.clamp(-max, max)
}

// ---- Spawning ----

/// Makes the villain `id` (just spawned) a colossus: its state and its weak
/// points.
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_colossus(
    world: &mut World,
    tick: u64,
    id: NetId,
    spot: usize,
    home: DVec3,
    yaw: f64,
    level: u8,
    heroes: usize,
) {
    let c = Colossus {
        yaw,
        aim: yaw,
        next_shell: tick + ticks(4.0),
        next_stomp: tick + ticks(4.0),
        next_pulse: tick,
        next_charge: tick + ticks(10.0),
        next_lance: tick + ticks(8.0),
        next_smash: tick + ticks(3.0),
        goal: home + forward(yaw) * 600.0,
        goal_at: tick,
        heroes: heroes.clamp(1, 255) as u8,
        ..Colossus::default()
    };
    if let Some(e) = entity_of(world, id) {
        world
            .entity_mut(e)
            .insert((c, Orientation(yaw_rotation(yaw))));
    }
    spawn_parts(world, tick, id, spot, home, level, heroes, &COLOSSUS_PARTS);
}

/// Spawns the weak points `parts` of robot `on`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_parts(
    world: &mut World,
    tick: u64,
    on: NetId,
    spot: usize,
    at: DVec3,
    level: u8,
    heroes: usize,
    parts: &[u8],
) {
    for &part in parts {
        let id = spawn_enemy(world, tick, EnemyKind::WeakPoint, level, spot, at, heroes);
        if let Some(e) = entity_of(world, id) {
            world.entity_mut(e).insert(Mount { on, part });
        }
    }
}

fn entity_of(world: &mut World, id: NetId) -> Option<Entity> {
    world
        .query::<(Entity, &Net)>()
        .iter(world)
        .find(|(_, n)| n.0 == id)
        .map(|(e, _)| e)
}

// ---- One tick ----

/// The machines act (after the other villains moved): sentries shield,
/// weak points ride their robots, the colossus walks and fights. Blows at
/// the city go into `city`.
pub(crate) fn act(
    actors: &mut [Actor],
    solids: &Obstacles,
    streets: Option<&Streets>,
    routes: &mut BTreeMap<NetId, (u64, Vec<DVec2>)>,
    tick: u64,
    fx: &mut Vec<Fx>,
    city: &mut Vec<(DVec3, f32, f32)>,
) {
    for i in 0..actors.len() {
        if actors[i].colossus.is_some() {
            let route = routes.entry(actors[i].id).or_default();
            colossus(actors, i, solids, streets, route, tick, fx, city);
        }
    }
    project_shields(actors, tick);
    // Weak points ride their robots; one just broken says so once.
    for i in 0..actors.len() {
        let Some(m) = actors[i].mount else {
            continue;
        };
        if actors[i].f.status == Status::Down && actors[i].f.until != u64::MAX {
            actors[i].f.until = u64::MAX;
            let at = actors[i].chest();
            fx.push(event(tick, FxKind::PartBroken, m.on, actors[i].id, at));
        }
        let Some(r) = actors.iter().position(|a| a.id == m.on) else {
            continue;
        };
        let robot = &actors[r];
        let at = match robot.colossus {
            Some(c) => colossus_part(robot.body.pos, &c, m.part),
            None => dish_point(robot.body.pos),
        };
        let vel = robot.body.vel;
        // A weak point's "chest" (where blows land) is the part itself.
        actors[i].body = Body {
            pos: at - DVec3::Y * (BODY_HEIGHT * 0.6),
            vel,
            grounded: false,
        };
    }
}

/// Is the part `part` of robot `on` still standing?
fn part_up(actors: &[Actor], on: NetId, part: u8) -> bool {
    actors
        .iter()
        .any(|a| a.mount == Some(Mount { on, part }) && a.f.fighting())
}

/// Robo-sentries whose dish stands shield every villain near them.
fn project_shields(actors: &mut [Actor], tick: u64) {
    let projectors: Vec<DVec3> = actors
        .iter()
        .filter(|a| a.enemy.is_some_and(|e| e.kind == EnemyKind::Sentry) && a.f.standing())
        .filter(|a| part_up(actors, a.id, DISH))
        .map(|a| a.body.pos)
        .collect();
    if projectors.is_empty() {
        return;
    }
    for a in actors.iter_mut() {
        let Some(en) = a.enemy else {
            continue;
        };
        if matches!(en.kind, EnemyKind::WeakPoint | EnemyKind::Colossus) || !a.f.fighting() {
            continue;
        }
        if projectors
            .iter()
            .any(|p| p.distance(a.body.pos) <= PROJECTOR_REACH)
        {
            a.f.shield_until = a.f.shield_until.max(tick + ticks(0.5));
        }
    }
}

/// Heroes (indices) standing near `at` and low enough to be hit from the
/// ground.
fn heroes_on_ground(actors: &[Actor], at: DVec3, reach: f64, ground: f64) -> Vec<usize> {
    (0..actors.len())
        .filter(|&k| {
            let a = &actors[k];
            let flat = DVec2::new(a.body.pos.x - at.x, a.body.pos.z - at.z).length();
            a.hero.is_some()
                && a.f.standing()
                && flat <= reach
                && a.body.pos.y - ground < CLEAR_HEIGHT
        })
        .collect()
}

/// The bit a hero has in a "hit already" mask.
fn bit(id: NetId) -> u64 {
    1u64 << (id.0 % 64)
}

/// Shortest distance from `p` to the segment `a`..`b`.
fn to_segment(p: DVec3, a: DVec3, b: DVec3) -> f64 {
    let ab = b - a;
    let t = if ab.length_squared() > 1e-9 {
        ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    p.distance(a + ab * t)
}

/// A blow of the colossus on hero `k` (`share` of its damage), told as `kind`.
fn blow(
    actors: &mut [Actor],
    i: usize,
    k: usize,
    share: f32,
    kind: FxKind,
    tick: u64,
    fx: &mut Vec<Fx>,
) {
    let spec = enemy_spec(EnemyKind::Colossus);
    let scale = level_scale(actors[i].f.level);
    let (_, defeat, dealt) = hit(actors, i, k, spec.damage * share * scale, false, tick);
    let mut e = event(tick, kind, actors[i].id, actors[k].id, actors[k].chest());
    e.amount = dealt.round() as u16;
    e.defeat = defeat;
    fx.push(e);
}

/// A building to head for, a little way off `at` (somewhere new to wreck).
fn pick_building(solids: &Obstacles, at: DVec3, seed: u64) -> Option<DVec3> {
    let here = DVec2::new(at.x, at.z);
    let near = solids.buildings_near(here, 220.0);
    if near.is_empty() {
        return None;
    }
    let mut rng = hw_core::Seed(seed).rng();
    (0..8).find_map(|_| {
        let i = near[(rng.next_u64() % near.len() as u64) as usize];
        let (lo, hi) = solids.building_bounds(i)?;
        let mid = (lo + hi) * 0.5;
        (40.0..220.0)
            .contains(&mid.distance(here))
            .then(|| DVec3::new(mid.x, at.y, mid.y))
    })
}

/// The building in its way: the point on it to ram, if one is within
/// [`SMASH_REACH`] ahead.
fn building_ahead(solids: &Obstacles, at: DVec3, yaw: f64) -> Option<DVec3> {
    let here = DVec2::new(at.x, at.z);
    let f = forward(yaw);
    let ahead = here + DVec2::new(f.x, f.z) * (SMASH_REACH * 0.5);
    solids
        .buildings_near(ahead, SMASH_REACH * 0.5)
        .into_iter()
        .filter_map(|i| {
            let (lo, hi) = solids.building_bounds(i)?;
            // The nearest point of its box, a little way in.
            let near = here.clamp(lo, hi);
            let mid = (lo + hi) * 0.5;
            let p = near + (mid - near).normalize_or_zero() * 2.0;
            let to = p - here;
            let d = to.length();
            let ahead = to.normalize_or_zero().dot(DVec2::new(f.x, f.z));
            (d <= SMASH_REACH && d > 3.0 && ahead > 0.5).then_some((d, p))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, p)| DVec3::new(p.x, at.y, p.y))
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn colossus(
    actors: &mut [Actor],
    i: usize,
    solids: &Obstacles,
    streets: Option<&Streets>,
    route: &mut (u64, Vec<DVec2>),
    tick: u64,
    fx: &mut Vec<Fx>,
    city: &mut Vec<(DVec3, f32, f32)>,
) {
    let Some(mut c) = actors[i].colossus else {
        return;
    };
    let me = actors[i].id;
    let here = actors[i].body.pos;
    let ground_of = |p: DVec3| solids.ground_at(DVec2::new(p.x, p.z)).max(GROUND_Y);
    let ground = ground_of(here);
    if actors[i].f.status == Status::Down {
        // Beaten: it slumps and smokes until it is hauled off.
        c.kneel = (c.kneel + DT * 0.8).min(1.0);
        c.shells = [None; SALVO];
        c.stomp = None;
        c.pulse = None;
        c.charge = None;
        c.lance = None;
        c.smash = None;
        c.wreck_at.get_or_insert(tick + ticks(WRECK_SECONDS));
        actors[i].body.vel = DVec3::ZERO;
        actors[i].f.marks = EFFECT_KNEELING;
        actors[i].colossus = Some(c);
        return;
    }
    let knees_down = (0..4u8).filter(|&k| !part_up(actors, me, k)).count();
    let cannon = part_up(actors, me, CANNON);
    // Down, up again, down for good.
    match c.down_at {
        None if (knees_down >= 2 && !c.risen) || knees_down >= 4 => {
            c.down_at = Some(tick);
            c.shells = [None; SALVO];
            c.stomp = None;
            c.charge = None;
            c.lance = None;
            c.smash = None;
            c.next_pulse = tick + ticks(3.0);
            c.drop = DROPS.saturating_mul(c.heroes).min(6);
            fx.push(event(tick, FxKind::Kneel, me, me, here));
        }
        Some(t) if !c.risen && knees_down < 4 && tick >= t + ticks(KNEEL_SECONDS) => {
            c.down_at = None;
            c.risen = true;
            c.pulse = None;
            c.next_shell = tick + ticks(3.0);
            c.next_charge = tick + ticks(6.0);
            c.next_lance = tick + ticks(9.0);
            fx.push(event(tick, FxKind::Rise, me, me, here));
        }
        _ => {}
    }
    let down = c.down_at.is_some();
    c.kneel = if down {
        (c.kneel + DT * 0.6).min(1.0)
    } else {
        (c.kneel - DT * 0.25).max(0.0)
    };
    let mut marks = 0;
    if c.kneel < 0.8 {
        marks |= EFFECT_ARMOURED;
    }
    if down {
        marks |= EFFECT_KNEELING;
    }
    if c.risen {
        marks |= EFFECT_ENRAGED;
    }
    actors[i].f.marks = marks;
    // Calm or enraged: how soon its moves come round again.
    let risen = c.risen;
    let pick = move |p: (f64, f64)| if risen { p.1 } else { p.0 };
    let up = !down && c.kneel < 0.05;

    // The hero to fight: the nearest standing one it can notice.
    let flat = |a: DVec3, b: DVec3| DVec2::new(b.x - a.x, b.z - a.z);
    let near_heroes: Vec<(usize, f64)> = (0..actors.len())
        .filter(|&k| actors[k].hero.is_some() && actors[k].f.standing())
        .map(|k| (k, flat(here, actors[k].body.pos).length()))
        .collect();
    let foe = near_heroes
        .iter()
        .copied()
        .filter(|(_, d)| *d < NOTICE)
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
        .map(|(k, _)| k);
    // Anyone watching (a hero who is down still sees it).
    let seen = ALWAYS_SEEN.load(Ordering::Relaxed)
        || actors
            .iter()
            .any(|a| a.hero.is_some() && flat(here, a.body.pos).length() < SEEN);
    actors[i].f.target = foe.map(|k| actors[k].id);
    if foe.is_some() {
        actors[i].f.last_combat = tick;
    }

    // A big move starts: a charge down a lane, the lance, or ramming a
    // building in its way. One at a time, and never while it reels.
    if up && !c.busy(tick) {
        let foe_at = foe.map(|k| {
            let p = actors[k].body.pos;
            (k, p, flat(here, p))
        });
        let low = |p: DVec3| p.y - ground_of(p) < 5.0;
        match foe_at {
            Some((k, p, to))
                if tick >= c.next_charge
                    && low(p)
                    && (CHARGE_RANGE.0..CHARGE_RANGE.1).contains(&to.length()) =>
            {
                let dir = to.normalize_or_zero();
                let run = (to.length() + CHARGE_OVER).min(CHARGE_MOST);
                let mut end = here + DVec3::new(dir.x, 0.0, dir.y) * run;
                end.y = ground_of(end);
                c.charge = Some(Charge {
                    go: tick + ticks(CHARGE_WINDUP),
                    from: here,
                    to: end,
                    hit: 0,
                });
                let mut e = event(tick, FxKind::ChargeWindup, me, actors[k].id, end);
                e.amount = run.round() as u16;
                fx.push(e);
            }
            Some((k, _, to))
                if cannon
                    && tick >= c.next_lance
                    && (LANCE_RANGE.0..LANCE_RANGE.1).contains(&to.length()) =>
            {
                let dir = to.normalize_or_zero();
                let dir3 = DVec3::new(dir.x, 0.0, dir.y);
                let reach = (to.length() + LANCE_OVER).min(LANCE_MOST);
                let mut near = here + dir3 * LANCE_NEAR;
                near.y = ground_of(near);
                let mut far = here + dir3 * reach;
                far.y = ground_of(far);
                let cannon_health = cannon_of(actors, me).map_or(0.0, |w| actors[w].f.health);
                c.lance = Some(Lance {
                    fire: tick + ticks(LANCE_WINDUP),
                    near,
                    far,
                    hit: 0,
                    cannon: cannon_health,
                    done: 0.0,
                });
                let mut e = event(tick, FxKind::LanceWindup, me, actors[k].id, far);
                e.amount = reach.round() as u16;
                fx.push(e);
            }
            _ if seen && tick >= c.next_smash => {
                if let Some(at) = building_ahead(solids, here, c.yaw) {
                    c.smash = Some((tick + ticks(SMASH_WINDUP), at));
                    fx.push(event(tick, FxKind::SmashWindup, me, me, at));
                }
            }
            _ => {}
        }
    }

    // Walking: after the hero it fights, straight through whatever is in
    // the way; with heroes in sight, from building to building; else down
    // the main roads (it is too wide for anything else).
    let mut vel = DVec3::ZERO;
    let mut charging = false;
    if let Some(mut ch) = c.charge.filter(|_| up) {
        let to = flat(here, ch.to);
        if to.length() > 0.5 {
            let fast = if tick >= ch.go { TURN } else { 90.0 };
            c.yaw = turn_towards(c.yaw, to.x.atan2(to.y), fast.to_radians() * DT);
        }
        let out_of_time = tick > ch.go + ticks(CHARGE_MOST / CHARGE_SPEED + 2.0);
        if tick >= ch.go && (to.length() < 1.0 || out_of_time) {
            c.charge = None;
            c.next_charge = tick + ticks(pick(CHARGE_EVERY));
        } else if tick >= ch.go {
            if tick == ch.go {
                fx.push(event(tick, FxKind::Charge, me, me, here));
            }
            charging = true;
            let speed = CHARGE_SPEED.min(to.length() / DT);
            vel = forward(c.yaw) * speed;
            c.walked += speed * DT;
            // Heroes in its lane are run down.
            for k in 0..actors.len() {
                let a = &actors[k];
                if a.hero.is_none() || !a.f.standing() || ch.hit & bit(a.id) != 0 {
                    continue;
                }
                let high = a.body.pos.y - ground_of(a.body.pos);
                if flat(here, a.body.pos).length() <= CHARGE_WIDTH && high < HULL_STAND + 4.0 {
                    ch.hit |= bit(a.id);
                    blow(actors, i, k, CHARGE_HITS, FxKind::Charge, tick, fx);
                }
            }
            c.charge = Some(ch);
        }
    } else if up && !c.planted(tick) {
        let goal = match foe {
            Some(k) => actors[k].body.pos,
            None => {
                if seen
                    && (tick >= c.goal_at + ticks(GOAL_SECONDS)
                        || flat(here, c.goal).length() < 6.0)
                {
                    if let Some(g) = pick_building(solids, here, tick ^ ((me.0 as u64) << 32)) {
                        c.goal = g;
                    }
                    c.goal_at = tick;
                }
                c.goal
            }
        };
        let near = flat(here, goal).length() <= if foe.is_some() { KEEP } else { 4.0 };
        let mut to = flat(here, goal);
        let mut far = !near;
        // Out of sight, along the streets.
        // (And for a while after a building of halls stood in its way:
        // the streets lead round the block.)
        let walled = c.skirt != 0 && tick < c.skirt_at + ticks(ROUND_SECONDS);
        let streets = streets.filter(|s| !s.is_empty() && (!seen || walled));
        if let Some(streets) = streets {
            let at = DVec2::new(here.x, here.z);
            let stale = tick >= route.0 + ticks(REPLAN);
            if route.1.is_empty() || stale {
                route.0 = tick;
                route.1 = streets
                    .route(at, DVec2::new(goal.x, goal.z))
                    .unwrap_or_default();
            }
            while route.1.len() > 1 && route.1[0].distance(at) < WAYPOINT {
                route.1.remove(0);
            }
            match route.1.first() {
                Some(&w) if w.distance(at) >= WAYPOINT * 0.5 => {
                    to = w - at;
                }
                _ => {
                    // At the end of the street route: stand, or pick a new
                    // street end to wander to.
                    far = false;
                    if foe.is_none() {
                        // Somewhere in town: a street point a few hundred
                        // metres off (country roads lead out of it).
                        let mut rng = hw_core::Seed(tick ^ actors[i].id.0 as u64).rng();
                        let pick = (0..16)
                            .map(|_| streets.node((rng.next_u64() % streets.len() as u64) as u32))
                            .find(|p| (150.0..WANDER).contains(&p.distance(at)));
                        if let Some(p) = pick {
                            c.goal = DVec3::new(p.x, here.y, p.y);
                        }
                        route.1.clear();
                    }
                }
            }
        } else {
            route.1.clear();
        }
        if to.length() > 0.5 {
            let want = to.x.atan2(to.y);
            c.yaw = turn_towards(c.yaw, want, TURN.to_radians() * DT);
        }
        if far {
            let facing = forward(c.yaw);
            let along = DVec2::new(facing.x, facing.z).dot(to.normalize_or_zero());
            let speed = if c.risen { LIMP_SPEED } else { WALK_SPEED } * along.clamp(0.3, 1.0);
            vel = facing * speed;
            c.walked += speed * DT;
        }
    }
    let mut pos = here + vel * DT;
    // A building of halls stands: its ground floor does not break, so the
    // tank walks round it (and rams what breaks above) rather than wading
    // in, where its core could not be seen or hit. (One that is in such a
    // house already walks out as it likes.)
    let plan_at = |p: DVec3| DVec2::new(p.x, p.z);
    let shut = |p: DVec3| solids.in_halls(plan_at(p), HALLS_CLEAR);
    // (It looks a metre ahead, so it does not walk into the wall and back.)
    let ahead = |v: DVec3| here + v.normalize_or_zero();
    if vel != DVec3::ZERO && (shut(pos) || shut(ahead(vel))) && !shut(here) {
        // Along the wall instead, to the side that is open (the same side
        // for as long as the wall lasts), and round the house's corner.
        let speed = vel.length();
        let f = vel / speed;
        let side = |s: f64| DVec3::new(f.z * s, 0.0, -f.x * s);
        if c.skirt == 0 || tick > c.skirt_at + ticks(2.0) {
            c.skirt = if shut(here + side(1.0) * 6.0) && !shut(here + side(-1.0) * 6.0) {
                -1
            } else {
                1
            };
        }
        c.skirt_at = tick;
        let tangent = side(f64::from(c.skirt));
        let slide = if shut(here + tangent * speed * DT) {
            (tangent - f * 0.3).normalize()
        } else {
            tangent
        };
        let along = here + slide * speed * DT;
        if shut(along) {
            // A corner between two houses: back the other way.
            c.skirt = -c.skirt;
            pos = here;
            vel = DVec3::ZERO;
        } else {
            pos = along;
            vel = slide * speed;
        }
        if charging {
            charging = false;
            c.charge = None;
            c.next_charge = tick + ticks(pick(CHARGE_EVERY));
        }
    }
    pos.y = ground_of(pos);
    actors[i].body = Body {
        pos,
        vel,
        grounded: true,
    };
    // Footfalls break what they land on; the hull ploughs through upper floors.
    // One foot lands every third of a stride (the client walks a cycle of
    // four footfalls in 4/3 strides).
    let quarter = STRIDE / 3.0;
    let steps = (c.walked / quarter).floor() as u64;
    if steps > c.steps {
        c.steps = steps;
        let k = STEP_ORDER[(steps % 4) as usize];
        let ahead = DVec3::Z * (STRIDE * 0.5);
        let foot = to_world(pos, c.yaw, foot_rest(k, 0.0) + ahead);
        city.push((foot + DVec3::Y * 1.0, 3.2, 20_000.0));
        let hull = hull_height(c.kneel);
        let front = to_world(pos, c.yaw, DVec3::new(0.0, hull + 1.8, 4.5));
        city.push((front, 5.0, 20_000.0));
        if charging {
            // Head down, it bulldozes a lane as wide as itself.
            for x in [-3.5, 0.0, 3.5] {
                let p = to_world(pos, c.yaw, DVec3::new(x, 3.5, 6.0));
                city.push((p, 4.0, 25_000.0));
            }
        }
    }

    // The lance: it charges, and an overload stops it; then it rakes the line.
    if let Some(mut l) = c.lance {
        let cannon_at = cannon_of(actors, me);
        if !cannon || down {
            c.lance = None;
        } else if tick < l.fire {
            let to = flat(pos, l.far);
            c.aim = turn_towards(c.aim, to.x.atan2(to.y), 60f64.to_radians() * DT);
            let now = cannon_at.map_or(0.0, |w| actors[w].f.health);
            let most = cannon_at.map_or(1.0, |w| actors[w].f.max_health);
            if l.cannon - now >= most * OVERLOAD_SHARE {
                // Overloaded: the beam fizzles in the barrel.
                c.lance = None;
                c.next_lance = tick + ticks(pick(LANCE_EVERY));
                c.stagger_until = tick + ticks(STAGGER_SECONDS);
                c.shells = [None; SALVO];
                c.stomp = None;
                if let Some(w) = cannon_at {
                    let a = &mut actors[w].f;
                    a.health = (a.health - a.max_health * OVERLOAD_BURN).max(1.0);
                }
                fx.push(event(tick, FxKind::Overload, me, me, cannon_point(pos, &c)));
            }
        } else {
            let span = LANCE_SECONDS;
            let t0 = (tick - l.fire) as f64 * DT / span;
            let t1 = t0 + DT / span;
            if tick == l.fire {
                fx.push(event(tick, FxKind::Lance, me, me, l.far));
            }
            let muzzle = muzzle_point(pos, &c);
            // Heroes the beam passes through this tick.
            for k in 0..actors.len() {
                let a = &actors[k];
                if a.hero.is_none() || !a.f.standing() || l.hit & bit(a.id) != 0 {
                    continue;
                }
                let chest = a.chest();
                let burnt = (0..=3).any(|n| {
                    let s = t0 + (t1 - t0) * n as f64 / 3.0;
                    to_segment(chest, muzzle, lance_point(&l, s)) <= LANCE_WIDTH
                });
                if burnt {
                    l.hit |= bit(a.id);
                    blow(actors, i, k, LANCE_HITS, FxKind::Lance, tick, fx);
                }
            }
            // The city along the line: the ground it scores and the floors
            // the beam cuts on its way down.
            let length = l.near.distance(l.far).max(1.0);
            let swept = t1.min(1.0) * length;
            while l.done + 3.0 <= swept {
                l.done += 3.0;
                let mut g = lance_point(&l, l.done / length);
                g.y = ground_of(g);
                city.push((g + DVec3::Y, 3.0, 16_000.0));
                let cut = g.y + 6.0;
                if muzzle.y > cut {
                    let u = (muzzle.y - cut) / (muzzle.y - g.y).max(0.1);
                    city.push((muzzle + (g - muzzle) * u, 2.5, 16_000.0));
                }
            }
            if t1 >= 1.0 {
                c.lance = None;
                c.next_lance = tick + ticks(pick(LANCE_EVERY));
            } else {
                c.lance = Some(l);
            }
        }
    }

    // Ramming a building: it rears, then drives into the wall.
    if let Some((lands, at)) = c.smash {
        if down {
            c.smash = None;
        } else if tick >= lands {
            c.smash = None;
            c.next_smash = tick
                + ticks(if foe.is_some() {
                    SMASH_EVERY.1
                } else {
                    SMASH_EVERY.0
                });
            let g = ground_of(at);
            for h in [1.5, 5.0, 9.0] {
                city.push((DVec3::new(at.x, g + h, at.z), 4.5, 30_000.0));
            }
            for k in heroes_on_ground(actors, at, SMASH_AREA, g) {
                blow(actors, i, k, SMASH_HITS, FxKind::Smash, tick, fx);
            }
            fx.push(event(tick, FxKind::Smash, me, me, at));
        }
    }

    // The cannon turns to its foe and shells it; enraged, in salvos.
    if c.lance.is_none() {
        let want = foe.map_or(c.yaw, |k| {
            let to = flat(pos, actors[k].body.pos);
            to.x.atan2(to.y)
        });
        c.aim = turn_towards(c.aim, want, 30f64.to_radians() * DT);
    }
    if !cannon || down {
        c.shells = [None; SALVO];
    }
    for n in 0..SALVO {
        let Some((lands, at)) = c.shells[n] else {
            continue;
        };
        if tick < lands {
            continue;
        }
        c.shells[n] = None;
        for k in 0..actors.len() {
            let a = &actors[k];
            if a.hero.is_none() || !a.f.standing() || a.chest().distance(at) > SHELL_AREA {
                continue;
            }
            blow(actors, i, k, 1.0, FxKind::Shell, tick, fx);
        }
        fx.push(event(tick, FxKind::Shell, me, me, at));
        city.push((at, 3.5, 12_000.0));
    }
    let loaded = c.shells.iter().all(Option::is_none);
    if loaded && cannon && !down && !c.busy(tick) && tick >= c.next_shell {
        if let Some(k) = foe {
            let d = flat(pos, actors[k].body.pos);
            if (SHELL_RANGE.0..SHELL_RANGE.1).contains(&d.length()) {
                let at = actors[k].chest();
                let across = DVec3::new(-d.y, 0.0, d.x).normalize_or_zero();
                let salvo = if c.risen { SALVO } else { 1 };
                for n in 0..salvo {
                    let off = [0.0, 1.0, -1.0][n] * SALVO_SPREAD;
                    let mut p = at + across * off;
                    if n > 0 {
                        p.y = ground_of(p) + 1.0;
                    }
                    let lands = tick + ticks(SHELL_WINDUP + SALVO_GAP * n as f64);
                    c.shells[n] = Some((lands, p));
                    fx.push(event(tick, FxKind::ShellWindup, me, actors[k].id, p));
                }
                c.next_shell = tick + ticks(SHELL_WINDUP + pick(SHELL_EVERY));
            }
        }
    }

    // Stomping heroes under it.
    if down || charging {
        c.stomp = None;
    }
    match c.stomp {
        Some(lands) if tick >= lands => {
            c.stomp = None;
            for k in heroes_on_ground(actors, pos, STOMP_AREA, pos.y) {
                blow(actors, i, k, STOMP_HITS, FxKind::Stomp, tick, fx);
            }
            fx.push(event(tick, FxKind::Stomp, me, me, pos));
            city.push((pos + DVec3::Y, STOMP_AREA as f32 * 0.5, 15_000.0));
        }
        None if !down
            && !c.busy(tick)
            && tick >= c.next_stomp
            && !heroes_on_ground(actors, pos, STOMP_AREA * 0.75, pos.y).is_empty() =>
        {
            c.stomp = Some(tick + ticks(STOMP_WINDUP));
            c.next_stomp = tick + ticks(STOMP_WINDUP + STOMP_EVERY);
            fx.push(event(tick, FxKind::StompWindup, me, me, pos));
        }
        _ => {}
    }

    // Down, the core discharges round it.
    if !down {
        c.pulse = None;
    }
    let core = core_point(pos, &c);
    match c.pulse {
        Some(lands) if tick >= lands => {
            c.pulse = None;
            for k in heroes_on_ground(actors, pos, PULSE_AREA, ground) {
                blow(actors, i, k, PULSE_HITS, FxKind::Pulse, tick, fx);
            }
            fx.push(event(tick, FxKind::Pulse, me, me, core));
        }
        None if down && c.kneel > 0.9 && tick >= c.next_pulse => {
            c.pulse = Some(tick + ticks(PULSE_WINDUP));
            c.next_pulse = tick + ticks(PULSE_WINDUP + pick(PULSE_EVERY));
            fx.push(event(tick, FxKind::PulseWindup, me, me, core));
        }
        _ => {}
    }
    actors[i].colossus = Some(c);
}

/// The actor index of robot `on`'s cannon, while it stands.
fn cannon_of(actors: &[Actor], on: NetId) -> Option<usize> {
    actors
        .iter()
        .position(|a| a.mount == Some(Mount { on, part: CANNON }) && a.f.fighting())
}

/// After a tick: weak points whose robot is gone go too, sentries drop from
/// a colossus's hatch, and a wreck is hauled off.
pub(crate) fn tidy(world: &mut World, tick: u64) {
    let robots: Vec<NetId> = world
        .query_filtered::<&Net, With<Enemy>>()
        .iter(world)
        .map(|n| n.0)
        .collect();
    let mut orphans: Vec<(NetId, Entity)> = world
        .query::<(Entity, &Net, &Mount)>()
        .iter(world)
        .filter(|(_, _, m)| !robots.contains(&m.on))
        .map(|(e, n, _)| (n.0, e))
        .collect();
    orphans.sort_by_key(|o| o.0);
    for (_, e) in orphans {
        world.despawn(e);
    }
    let mut tanks: Vec<(NetId, Entity, Colossus, DVec3, Enemy, u8)> = world
        .query::<(
            Entity,
            &Net,
            &Colossus,
            &Motion,
            &Enemy,
            &crate::combat::Fighter,
        )>()
        .iter(world)
        .map(|(e, n, c, m, en, f)| (n.0, e, *c, m.0.pos, *en, f.level))
        .collect();
    tanks.sort_by_key(|t| t.0);
    for (_, e, mut c, at, en, level) in tanks {
        if c.wreck_at.is_some_and(|t| tick >= t) {
            world.despawn(e);
            let mut f = world.resource_mut::<Fights>();
            let s = &mut f.spots[en.spot];
            s.members = s.members.saturating_sub(1);
            continue;
        }
        if c.drop == 0 {
            continue;
        }
        let n = c.drop;
        c.drop = 0;
        world.entity_mut(e).insert(c);
        let solids = world.resource::<Solids>().0.clone();
        for k in 0..n {
            let a = (k as f64 / n as f64) * std::f64::consts::TAU + c.yaw;
            let p = DVec2::new(at.x + a.sin() * 7.0, at.z + a.cos() * 7.0);
            let home = DVec3::new(p.x, solids.ground_at(p).max(GROUND_Y), p.y);
            let id = spawn_enemy(world, tick, EnemyKind::Sentry, level, en.spot, home, 1);
            spawn_parts(world, tick, id, en.spot, home, level, 1, &[DISH]);
        }
        world.resource_mut::<Fights>().spots[en.spot].members += n as u32;
    }
}
