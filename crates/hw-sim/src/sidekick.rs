//! A hero's robot sidekick (the Robot Sidekick power): a small robot that
//! keeps at its hero's side, fights the villains its hero fights (or that
//! go for either of them), and can be sent at a target with the power's
//! key. Villains fight it like a hero. Beaten, it powers down and is taken
//! away; the power recharges and calls a fresh one.
//!
//! It walks with the same movement as people; left far behind, or stuck
//! behind a wall, it beams back to its hero's side.

use crate::collision::Body;
use crate::combat::{self, Fighter, Fights};
use crate::{step_moving, LastChange, Motion, Moves, Net, Orientation, Solids, DT};
use bevy_ecs::prelude::*;
use glam::{DVec2, DVec3};
use hw_core::NetId;
use hw_net::protocol::FxKind;
use hw_net::{InputState, PlayerId, PowerId, Status};

/// Its punch at level 1, its reach and how often (m, s).
pub const PUNCH_DAMAGE: f32 = 14.0;
pub const PUNCH_REACH: f64 = 1.7;
pub const PUNCH_EVERY: f64 = 1.1;
/// It goes for villains attacking its hero (or it) this near (m).
pub const GUARD_REACH: f64 = 18.0;
/// Its health as a share of its hero's.
pub const HEALTH_SHARE: f32 = 0.6;
/// Where it keeps: this far behind and beside its hero (m).
pub const FOLLOW: f64 = 2.2;
/// Its hero standing, it rests anywhere this near and far (m).
pub const SETTLE_NEAR: f64 = 1.2;
pub const SETTLE_FAR: f64 = 3.4;
/// Its hero counts as on the move above this level speed (m/s).
pub const HERO_MOVING: f64 = 0.5;
/// How much faster it goes per metre it is behind its place (1/s).
pub const CATCH_UP: f64 = 1.6;
/// Further than this from its hero, it beams back (m).
pub const BEAM_BACK: f64 = 30.0;
/// It beams back after this long without getting any nearer (s).
pub const STUCK_SECONDS: f64 = 2.0;
/// Beaten, it lies there this long before it is taken away (s).
pub const DOWN_SECONDS: f64 = 3.0;
/// How big it is drawn, as a share of a grown person.
pub const SIZE: f32 = 0.62;

/// A hero's sidekick.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Sidekick {
    /// The hero it belongs to, and the hero's player.
    pub owner: NetId,
    pub player: PlayerId,
    /// The power that called it, and how strong it is (its punch and
    /// health, as a share of an ordinary one's).
    pub power: PowerId,
    pub strength: f32,
    /// Its look (0: a tin robot; 1: a gold one).
    pub theme: u8,
    /// Ticks spent without getting nearer to where it is going, and the
    /// best distance in that time.
    pub stuck: u32,
    pub best: f64,
}

/// Calls a fresh sidekick for `sm.owner` at `sm.at`.
pub(crate) fn spawn(world: &mut World, sm: combat::Summon, tick: u64) -> NetId {
    let combat::Summon {
        owner,
        player,
        power,
        index,
        at,
        level,
    } = sm;
    let strength = crate::powers::def_of(power).fields.summon_power.max(0.1);
    // The first of a call sends old ones (still lying there, or another
    // power's) home.
    let old: Vec<Entity> = world
        .query::<(Entity, &Sidekick)>()
        .iter(world)
        .filter(|(_, s)| s.owner == owner && index == 0)
        .map(|(e, _)| e)
        .collect();
    for e in old {
        world.despawn(e);
    }
    let id = world.resource_mut::<crate::Ids>().0.allocate();
    let mut f = Fighter::hero_at(level, combat::Kit::default());
    f.max_health *= HEALTH_SHARE * strength;
    f.health = f.max_health;
    world.spawn((
        Net(id),
        Motion(Body {
            pos: at,
            vel: DVec3::ZERO,
            grounded: false,
        }),
        Orientation(hw_net::protocol::NO_ROTATION),
        f,
        Sidekick {
            owner,
            player,
            power,
            strength,
            theme: (power.0 % 2),
            stuck: 0,
            best: f64::INFINITY,
        },
        LastChange(tick),
    ));
    id
}

/// One tick of every sidekick: walk to the fight or to its hero's side,
/// beam back when left behind, go when beaten or no longer wanted.
pub fn step(world: &mut World, tick: u64) {
    let solids = world.resource::<Solids>().0.clone();
    // Heroes: where, which way they face, and whether they still own it.
    let heroes: Vec<(NetId, DVec3, DVec3, combat::Kit, u8)> = world
        .query::<(&Net, &Motion, &Fighter, &crate::Hero)>()
        .iter(world)
        .map(|(n, m, f, _)| (n.0, m.0.pos, m.0.vel, f.kit, f.level))
        .collect();
    let fighters: Vec<(NetId, DVec3)> = world
        .query::<(&Net, &Motion, &Fighter)>()
        .iter(world)
        .map(|(n, m, _)| (n.0, m.0.pos))
        .collect();
    let mut gone = Vec::new();
    let mut fx = Vec::new();
    let mut q = world.query::<(Entity, &Net, &mut Motion, &mut Fighter, &mut Sidekick)>();
    let mut list: Vec<_> = q.iter_mut(world).collect();
    list.sort_by_key(|x| x.1 .0);
    for (e, net, mut motion, mut f, mut sk) in list {
        let Some(&(_, hero, hero_vel, kit, level)) = heroes.iter().find(|h| h.0 == sk.owner) else {
            gone.push(e);
            continue;
        };
        let owns = kit.has(sk.power);
        if !owns {
            gone.push(e);
            continue;
        }
        if f.status == Status::Down {
            // Lies there a moment, then is taken away.
            f.until = f.until.min(tick + combat::ticks(DOWN_SECONDS));
            if tick >= f.until {
                gone.push(e);
            }
            let mut body = motion.0;
            step_moving(&mut body, InputState::default(), &solids, Moves::ON_FOOT);
            motion.set_if_neq(Motion(body));
            continue;
        }
        if f.level != level {
            f.set_level(level);
            f.max_health *= HEALTH_SHARE * sk.strength;
            f.health = f.health.min(f.max_health);
        }
        let mut body = motion.0;
        // Where to go: up to the villain it fights, else after its hero.
        let foe = f
            .target
            .and_then(|t| fighters.iter().find(|x| x.0 == t))
            .map(|x| x.1);
        let hero_flat = DVec3::new(hero_vel.x, 0.0, hero_vel.z);
        let hero_speed = hero_flat.length();
        let resting = DVec2::new(body.vel.x, body.vel.z).length() < 0.3;
        let place = keep_place(hero, hero_flat, body.pos, resting);
        let (goal, stop) = match foe {
            Some(v) => (v, PUNCH_REACH - 0.3),
            None => (place, 0.4),
        };
        let to = DVec2::new(goal.x - body.pos.x, goal.z - body.pos.z);
        let dist = to.length();
        // Left behind or stuck: beam back to the hero's side.
        if dist + 0.5 < sk.best {
            sk.best = dist;
            sk.stuck = 0;
        } else if dist > stop + 1.0 {
            sk.stuck += 1;
        }
        let far = body.pos.distance(hero) > BEAM_BACK;
        let stuck = sk.stuck as f64 * DT > STUCK_SECONDS;
        if far || stuck {
            let at = beam_spot(&solids, hero, place);
            body = Body {
                pos: at,
                vel: DVec3::ZERO,
                grounded: false,
            };
            sk.stuck = 0;
            sk.best = f64::INFINITY;
            let mut x = combat::event(tick, FxKind::SidekickCall, sk.owner, net.0, at);
            x.theme = sk.power.0;
            fx.push(x);
            motion.set_if_neq(Motion(body));
            continue;
        }
        let mut input = InputState::default();
        // Held or frozen: it cannot move either.
        let can_move = f.standing() && !f.held(tick) && !f.frozen(tick);
        if dist > stop && can_move {
            let d = to / dist;
            // It keeps its hero's pace, and hurries to catch up (little
            // legs, big motor); near its place it slows to a walk.
            let keep_up = if foe.is_none() { hero_speed } else { 0.0 };
            let want = keep_up + (dist - stop) * CATCH_UP;
            let pace = (want / crate::RUN_SPEED).clamp(0.15, 1.0);
            input.move_x = (d.x * pace) as f32;
            input.move_z = (d.y * pace) as f32;
        }
        if foe.is_none() && dist <= stop {
            sk.best = f64::INFINITY;
        }
        let moves = Moves {
            speed: dist > 8.0,
            ..Moves::ON_FOOT
        };
        step_moving(&mut body, input, &solids, moves);
        motion.set_if_neq(Motion(body));
    }
    for e in gone {
        world.despawn(e);
    }
    world.resource_mut::<Fights>().events.extend(fx);
}

/// Where a sidekick at `at` wants to be, its hero at `hero` going at
/// `hero_flat` (level velocity). On the move it trails behind and a little
/// to the right; once the hero stands, it walks in (or out) along the
/// straight line to them, so it never circles them, and once it has come
/// to rest (`resting`) it stays put anywhere in a comfortable ring
/// ([`SETTLE_NEAR`]..[`SETTLE_FAR`]).
pub fn keep_place(hero: DVec3, hero_flat: DVec3, at: DVec3, resting: bool) -> DVec3 {
    if hero_flat.length() > HERO_MOVING {
        let ahead = hero_flat.normalize();
        let right = DVec3::new(-ahead.z, 0.0, ahead.x);
        return hero - ahead * FOLLOW + right * (FOLLOW * 0.45);
    }
    let off = DVec3::new(at.x - hero.x, 0.0, at.z - hero.z);
    let d = off.length();
    let ring = if resting {
        SETTLE_NEAR..=SETTLE_FAR
    } else {
        FOLLOW - 0.4..=FOLLOW + 0.4
    };
    if ring.contains(&d) {
        return at;
    }
    hero + off.normalize_or(DVec3::NEG_Z) * FOLLOW
}

/// Where a sidekick beams in near its hero at `hero`: its place `want` if
/// a body fits there, else right beside the hero.
fn beam_spot(solids: &crate::collision::Obstacles, hero: DVec3, want: DVec3) -> DVec3 {
    for p in [want, hero + DVec3::X * 0.8, hero - DVec3::X * 0.8, hero] {
        let flat = DVec2::new(p.x, p.z);
        let floor = solids.floor_under(flat, hero.y + 1.0);
        let at = DVec3::new(p.x, floor.max(hero.y - 6.0), p.z);
        let fits = [0.2, 0.6, 1.0]
            .iter()
            .all(|h| !solids.solid_at(at + DVec3::Y * *h));
        if fits {
            return at + DVec3::Y * 0.02;
        }
    }
    hero
}
