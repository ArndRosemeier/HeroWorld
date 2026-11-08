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
//! The colossus is a set piece in three acts:
//! 1. It walks, armoured (blows glance off), shelling heroes with its cannon
//!    and stomping those under it; its feet and hull break what they walk
//!    into. The four glowing knee actuators and the cannon are weak points.
//! 2. With two knees gone it goes down on its belly: the clamshell opens, the
//!    Aether core shows and takes full damage, robo-sentries drop from the
//!    hatch, and the core discharges round it (fly or jump clear).
//! 3. After [`KNEEL_SECONDS`] it hauls itself up on two legs, enraged (faster
//!    shells). Break the other two knees and it goes down for good.
//!
//! Beaten, its wreck smokes a while and is hauled off.

use crate::collision::{Body, Obstacles, BODY_HEIGHT};
use crate::combat::{
    enemy_spec, event, hit, level_scale, spawn_enemy, ticks, Actor, Enemy, EnemyKind, Fights,
};
use crate::{Motion, Net, Orientation, Solids, DT, GROUND_Y};
use bevy_ecs::prelude::*;
use glam::{DVec2, DVec3};
use hw_core::NetId;
use hw_net::{Fx, FxKind, Status, EFFECT_ARMOURED, EFFECT_ENRAGED, EFFECT_KNEELING};

/// The colossus's core health (before the level and the team size).
pub const CORE_HEALTH: f32 = 1300.0;
/// Share of a blow that gets through shut armour.
pub const ARMOUR_LETS_THROUGH: f32 = 0.1;
/// Seconds it lies on its belly the first time before it gets up again.
pub const KNEEL_SECONDS: f64 = 25.0;
/// Seconds its wreck lies before it is hauled off.
pub const WRECK_SECONDS: f64 = 45.0;
/// Walking speed (m/s), and after its second wind.
pub const WALK_SPEED: f64 = 1.7;
const LIMP_SPEED: f64 = 1.1;
/// Turning (degrees a second).
const TURN: f64 = 14.0;
/// How near it comes to the hero it fights (m).
const KEEP: f64 = 15.0;
/// It goes for heroes this near (m), else walks on towards its goal.
const NOTICE: f64 = 160.0;
/// Shells: the gap between them (s), calm and enraged, the warning before
/// one lands (s), its blast (m), the cannon's reach (m).
const SHELL_EVERY: (f64, f64) = (7.0, 4.5);
pub const SHELL_WINDUP: f64 = 1.8;
pub const SHELL_AREA: f64 = 5.0;
const SHELL_RANGE: (f64, f64) = (10.0, 140.0);
/// The stomp: on heroes under it, every so often, after a warning.
const STOMP_EVERY: f64 = 6.0;
const STOMP_WINDUP: f64 = 1.2;
pub const STOMP_AREA: f64 = 12.0;
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

/// The walking tank's own state.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
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
    /// A shell on its way: (lands, where).
    pub shell: Option<(u64, DVec3)>,
    pub next_shell: u64,
    pub stomp: Option<u64>,
    pub next_stomp: u64,
    pub pulse: Option<u64>,
    pub next_pulse: u64,
    /// Where it marches when no hero is near.
    pub goal: DVec3,
    /// Beaten: hauled off at this tick.
    pub wreck_at: Option<u64>,
    /// Robo-sentries to drop from the hatch.
    pub drop: u8,
    /// The team it was sized for.
    pub heroes: u8,
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
pub const FOOT_OUT: f64 = 8.6;
pub const FOOT_SPRAWL: f64 = 10.4;
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

/// Direction of leg `k` from the hull's middle, in hull space (x, z):
/// 45 degrees round from the front, a quarter turn apart.
pub fn leg_dir(k: usize) -> DVec2 {
    let a = (45.0 + 90.0 * k as f64).to_radians();
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
        walked: 0.0,
        steps: 0,
        kneel: 0.0,
        down_at: None,
        risen: false,
        shell: None,
        next_shell: tick + ticks(5.0),
        stomp: None,
        next_stomp: tick + ticks(4.0),
        pulse: None,
        next_pulse: tick,
        goal: home + forward(yaw) * 600.0,
        wreck_at: None,
        drop: 0,
        heroes: heroes.clamp(1, 255) as u8,
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
    tick: u64,
    fx: &mut Vec<Fx>,
    city: &mut Vec<(DVec3, f32, f32)>,
) {
    for i in 0..actors.len() {
        if actors[i].colossus.is_some() {
            colossus(actors, i, solids, tick, fx, city);
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

fn colossus(
    actors: &mut [Actor],
    i: usize,
    solids: &Obstacles,
    tick: u64,
    fx: &mut Vec<Fx>,
    city: &mut Vec<(DVec3, f32, f32)>,
) {
    let Some(mut c) = actors[i].colossus else {
        return;
    };
    let me = actors[i].id;
    let here = actors[i].body.pos;
    let ground = solids.ground_at(DVec2::new(here.x, here.z)).max(GROUND_Y);
    if actors[i].f.status == Status::Down {
        // Beaten: it slumps and smokes until it is hauled off.
        c.kneel = (c.kneel + DT * 0.8).min(1.0);
        c.shell = None;
        c.stomp = None;
        c.pulse = None;
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
            c.shell = None;
            c.stomp = None;
            c.next_pulse = tick + ticks(3.0);
            c.drop = DROPS.saturating_mul(c.heroes).min(6);
            fx.push(event(tick, FxKind::Kneel, me, me, here));
        }
        Some(t) if !c.risen && knees_down < 4 && tick >= t + ticks(KNEEL_SECONDS) => {
            c.down_at = None;
            c.risen = true;
            c.pulse = None;
            c.next_shell = tick + ticks(3.0);
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
    let spec = enemy_spec(EnemyKind::Colossus);
    let scale = level_scale(actors[i].f.level);

    // The hero to fight: the nearest standing one it can notice.
    let flat = |a: DVec3, b: DVec3| DVec2::new(b.x - a.x, b.z - a.z);
    let foe = (0..actors.len())
        .filter(|&k| actors[k].hero.is_some() && actors[k].f.standing())
        .map(|k| (k, flat(here, actors[k].body.pos).length()))
        .filter(|(_, d)| *d < NOTICE)
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
        .map(|(k, _)| k);
    actors[i].f.target = foe.map(|k| actors[k].id);
    if foe.is_some() {
        actors[i].f.last_combat = tick;
    }

    // Walking: towards the hero it fights, else on towards its goal.
    let mut vel = DVec3::ZERO;
    if !down && c.kneel < 0.05 {
        let goal = foe.map_or(c.goal, |k| actors[k].body.pos);
        let to = flat(here, goal);
        let far = to.length() > if foe.is_some() { KEEP } else { 4.0 };
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
    pos.y = solids.ground_at(DVec2::new(pos.x, pos.z)).max(GROUND_Y);
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
    }

    // The cannon turns to its foe and shells it.
    if let Some(k) = foe {
        let to = flat(pos, actors[k].body.pos);
        c.aim = turn_towards(c.aim, to.x.atan2(to.y), 30f64.to_radians() * DT);
    } else {
        c.aim = turn_towards(c.aim, c.yaw, 30f64.to_radians() * DT);
    }
    if !cannon || down {
        c.shell = None;
    }
    match c.shell {
        Some((lands, at)) if tick >= lands => {
            c.shell = None;
            for k in 0..actors.len() {
                let a = &actors[k];
                if a.hero.is_none() || !a.f.standing() || a.chest().distance(at) > SHELL_AREA {
                    continue;
                }
                let (_, defeat, dealt) = hit(actors, i, k, spec.damage * scale, false, tick, None);
                let mut e = event(tick, FxKind::Shell, me, actors[k].id, actors[k].chest());
                e.amount = dealt.round() as u16;
                e.defeat = defeat;
                fx.push(e);
            }
            fx.push(event(tick, FxKind::Shell, me, me, at));
            city.push((at, 3.5, 12_000.0));
        }
        None if cannon && !down && tick >= c.next_shell => {
            if let Some(k) = foe {
                let d = flat(pos, actors[k].body.pos).length();
                if (SHELL_RANGE.0..SHELL_RANGE.1).contains(&d) {
                    let at = actors[k].chest();
                    c.shell = Some((tick + ticks(SHELL_WINDUP), at));
                    let gap = if c.risen {
                        SHELL_EVERY.1
                    } else {
                        SHELL_EVERY.0
                    };
                    c.next_shell = tick + ticks(SHELL_WINDUP + gap);
                    fx.push(event(tick, FxKind::ShellWindup, me, actors[k].id, at));
                }
            }
        }
        _ => {}
    }

    // Stomping heroes under it.
    if down {
        c.stomp = None;
    }
    match c.stomp {
        Some(lands) if tick >= lands => {
            c.stomp = None;
            for k in heroes_on_ground(actors, pos, STOMP_AREA, pos.y) {
                let (_, defeat, dealt) =
                    hit(actors, i, k, spec.damage * 0.8 * scale, false, tick, None);
                let mut e = event(tick, FxKind::Stomp, me, actors[k].id, actors[k].chest());
                e.amount = dealt.round() as u16;
                e.defeat = defeat;
                fx.push(e);
            }
            fx.push(event(tick, FxKind::Stomp, me, me, pos));
        }
        None if !down
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
                let (_, defeat, dealt) =
                    hit(actors, i, k, spec.damage * 0.9 * scale, false, tick, None);
                let mut e = event(tick, FxKind::Pulse, me, actors[k].id, actors[k].chest());
                e.amount = dealt.round() as u16;
                e.defeat = defeat;
                fx.push(e);
            }
            fx.push(event(tick, FxKind::Pulse, me, me, core));
        }
        None if down && c.kneel > 0.9 && tick >= c.next_pulse => {
            c.pulse = Some(tick + ticks(PULSE_WINDUP));
            let gap = if c.risen {
                PULSE_EVERY.1
            } else {
                PULSE_EVERY.0
            };
            c.next_pulse = tick + ticks(PULSE_WINDUP + gap);
            fx.push(event(tick, FxKind::PulseWindup, me, me, core));
        }
        _ => {}
    }
    actors[i].colossus = Some(c);
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
