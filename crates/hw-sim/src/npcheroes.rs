//! The town's own heroes. Five of them keep Hero Hall: they stand on its
//! plaza and greet newcomers with a tip, and when trouble goes unanswered
//! for a while (nobody near), the nearest one at home flies out, bolts the
//! villains from the air and flies home again. They are never beaten: hurt
//! badly, they break off and go home to mend. Villains they beat give no
//! one karma, and they leave anything a player hero is near to the player.

use crate::combat::{self, Enemy, Fighter, Fights};
use crate::incidents::{Incident, IncidentKind};
use crate::Body;
use crate::{LastChange, Motion, Net, Orientation, DT};
use bevy_ecs::prelude::*;
use glam::{DVec2, DVec3};
use hw_core::NetId;
use hw_net::protocol::FxKind;
use std::collections::BTreeMap;

/// Their names, by index.
pub const NAMES: [&str; 5] = [
    "Commander Starling",
    "Atomic Annie",
    "The Brass Badger",
    "Lady Lumen",
    "Rocket Rory",
];

/// What they tell newcomers, one at a time, in this order.
pub const TIPS: [&str; 10] = [
    "Welcome to Hero Hall, friend! Listen to your wrist radio: it calls when trouble starts.",
    "Tab picks out the nearest villain. Hit back when they come at you.",
    "Not every gang is out to get you. Leave them be and most will leave you be.",
    "Beat a gang's people and that gang remembers. Its rivals remember too, kindly.",
    "The mole folk are only frightened. Hold E near them and talk them down.",
    "Sky pirates moor over the harbour. Their lookouts warn you off before they fight.",
    "The Iron Legion is no joke. Bring a friend, and free the people they hold.",
    "Karma buys powers: open them with B. Every level is worth a new one.",
    "Hurt? Stand back a moment and you mend. Friends can lift you up with E.",
    "Taxis fly you across town. Look for the pads, or call one from the map.",
];

/// How near a newcomer comes before they are greeted, and how long until
/// the same hero greets them again (m, s).
pub const GREET_REACH: f64 = 7.0;
pub const GREET_AGAIN: f64 = 90.0;
/// Trouble nobody has come to for this long brings one of them (s), if no
/// player hero is within the incident's reward reach.
pub const ANSWER_AFTER: f64 = 75.0;
/// Flying speed, how high above the goal they cruise, and how high they
/// hover over a fight (m/s, m, m).
pub const FLY_SPEED: f64 = 28.0;
pub const CRUISE: f64 = 70.0;
pub const HOVER: f64 = 3.5;
/// Their bolt: reach, how often, and its damage at level 1 (m, s).
pub const BOLT_RANGE: f64 = 30.0;
pub const BOLT_EVERY: f64 = 1.5;
pub const BOLT_DAMAGE: f32 = 14.0;
/// Below this share of health they break off and go home.
pub const RETREAT_AT: f32 = 0.3;
/// Back to this share before they answer again.
pub const READY_AT: f32 = 0.8;
/// Their level.
pub const LEVEL: u8 = 12;
/// Health mended per second at home (a share of the most).
const MEND: f32 = 0.04;

/// One of the town's heroes.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct NpcHero {
    pub index: u8,
    /// Where they stand on the plaza, and which way they face.
    pub home: DVec3,
    pub facing: DVec2,
    pub job: Job,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Job {
    /// On the plaza.
    Home,
    /// Flying to (or fighting at) an incident.
    Answering { incident: u32, at: DVec3 },
    /// On the way home.
    Returning,
}

/// Who was greeted by whom and when, and how many tips each newcomer had.
#[derive(Resource, Default)]
pub struct Greetings {
    last: BTreeMap<(NetId, NetId), u64>,
    given: BTreeMap<NetId, u32>,
}

/// Puts the town's heroes on their spots (Hero Hall's plaza).
pub fn spawn(world: &mut World, tick: u64, spots: &[(DVec3, DVec2)]) {
    world.init_resource::<Greetings>();
    for (k, &(home, facing)) in spots.iter().take(NAMES.len()).enumerate() {
        let id = world.resource_mut::<crate::Ids>().0.allocate();
        let mut f = Fighter::hero_at(LEVEL, combat::Kit::default());
        f.health = f.max_health;
        world.spawn((
            Net(id),
            Motion(Body {
                pos: home,
                vel: DVec3::ZERO,
                grounded: true,
            }),
            Orientation(yaw_quat(facing)),
            f,
            combat::CombatView(f.view(tick)),
            NpcHero {
                index: k as u8,
                home,
                facing,
                job: Job::Home,
            },
            LastChange(tick),
        ));
    }
}

fn yaw_quat(dir: DVec2) -> [f32; 4] {
    glam::Quat::from_rotation_y(hw_core::det::atan2(dir.x, dir.y) as f32).to_array()
}

/// One tick: greet, choose trouble to answer, fly, pick targets (the bolts
/// themselves are thrown in the fight step).
pub fn step(world: &mut World, tick: u64, open: &[Incident]) {
    if !world.contains_resource::<Greetings>() {
        return;
    }
    let players: Vec<(NetId, DVec3)> = world
        .query::<(&Net, &Motion, &crate::Hero)>()
        .iter(world)
        .map(|(n, m, _)| (n.0, m.0.pos))
        .collect();
    let villains: Vec<(NetId, usize, DVec3, bool)> = world
        .query::<(&Net, &Motion, &Enemy, &Fighter)>()
        .iter(world)
        .map(|(n, m, e, f)| (n.0, e.spot, m.0.pos, f.fighting()))
        .collect();
    let mut npcs: Vec<(Entity, NetId, NpcHero, Body, Fighter)> = world
        .query::<(Entity, &Net, &NpcHero, &Motion, &Fighter)>()
        .iter(world)
        .map(|(e, n, h, m, f)| (e, n.0, *h, m.0, *f))
        .collect();
    npcs.sort_by_key(|x| x.1);
    let mut fx = Vec::new();

    // Trouble to answer, once a second: the oldest fight left alone, for
    // the nearest one at home and fit.
    if tick.is_multiple_of(crate::TICK_HZ as u64) {
        for inc in open {
            if !wants_help(inc, tick, &players) {
                continue;
            }
            let taken = npcs
                .iter()
                .any(|n| matches!(n.2.job, Job::Answering { incident, .. } if incident == inc.id));
            if taken {
                continue;
            }
            let pick = npcs
                .iter()
                .enumerate()
                .filter(|(_, n)| n.2.job == Job::Home && n.4.health >= n.4.max_health * READY_AT)
                // The story's contact keeps the hall.
                .filter(|(_, n)| n.2.index != crate::story::CONTACT)
                .min_by(|a, b| {
                    let da = a.1 .3.pos.distance(inc.at);
                    let db = b.1 .3.pos.distance(inc.at);
                    da.total_cmp(&db).then(a.1 .1.cmp(&b.1 .1))
                })
                .map(|(i, _)| i);
            if let Some(i) = pick {
                npcs[i].2.job = Job::Answering {
                    incident: inc.id,
                    at: inc.at,
                };
            }
        }
    }

    let mut greetings = world.remove_resource::<Greetings>().unwrap_or_default();
    for (_, id, hero, body, f) in npcs.iter_mut() {
        match hero.job {
            Job::Home => {
                *body = Body {
                    pos: hero.home,
                    vel: DVec3::ZERO,
                    grounded: true,
                };
                f.target = None;
                f.health = (f.health + f.max_health * MEND * DT as f32).min(f.max_health);
                // Newcomers on the plaza get a word, one tip after another.
                for &(p, at) in &players {
                    if at.distance(hero.home) > GREET_REACH {
                        continue;
                    }
                    let key = (*id, p);
                    let due = greetings
                        .last
                        .get(&key)
                        .is_none_or(|&t| tick.saturating_sub(t) as f64 * DT >= GREET_AGAIN);
                    // One of them speaks at a time.
                    let busy = greetings
                        .last
                        .iter()
                        .any(|(&(_, q), &t)| q == p && tick.saturating_sub(t) as f64 * DT < 6.0);
                    if due && !busy {
                        let n = greetings.given.entry(p).or_insert(0);
                        let mut e =
                            combat::event(tick, FxKind::Tip, *id, p, hero.home + DVec3::Y * 1.8);
                        e.amount = (*n as usize % TIPS.len()) as u16;
                        *n += 1;
                        fx.push(e);
                        greetings.last.insert(key, tick);
                    }
                }
            }
            Job::Answering { incident, at } => {
                let inc = open.iter().find(|i| i.id == incident);
                let spot = inc.and_then(|i| i.spot);
                let foes: Vec<&(NetId, usize, DVec3, bool)> = villains
                    .iter()
                    .filter(|v| Some(v.1) == spot && v.3)
                    .collect();
                // Done, a player came, or too hurt to go on: home.
                let player_near = players
                    .iter()
                    .any(|(_, p)| p.distance(at) < crate::incidents::REWARD);
                if inc.is_none()
                    || foes.is_empty()
                    || player_near
                    || f.health < f.max_health * RETREAT_AT
                {
                    hero.job = Job::Returning;
                    f.target = None;
                    continue;
                }
                // Over the fight, a little to one side each.
                let side = hero.facing.perp() * (hero.index as f64 - 2.0) * 2.5;
                let goal = at + DVec3::new(side.x, HOVER, side.y);
                fly(body, goal);
                // The nearest of them within reach is the target.
                f.target = foes
                    .iter()
                    .map(|v| (v.0, v.2.distance(body.pos)))
                    .filter(|(_, d)| *d <= BOLT_RANGE)
                    .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
                    .map(|(v, _)| v);
            }
            Job::Returning => {
                f.target = None;
                if fly(body, hero.home) {
                    hero.job = Job::Home;
                }
            }
        }
    }
    greetings
        .last
        .retain(|_, t| tick.saturating_sub(*t) as f64 * DT < GREET_AGAIN * 4.0);
    world.insert_resource(greetings);
    for (e, _, hero, body, f) in npcs {
        let mut ent = world.entity_mut(e);
        if let Some(mut h) = ent.get_mut::<NpcHero>() {
            if *h != hero {
                *h = hero;
            }
        }
        if let Some(mut m) = ent.get_mut::<Motion>() {
            if m.0 != body {
                m.0 = body;
            }
        }
        if let Some(mut fi) = ent.get_mut::<Fighter>() {
            *fi = f;
        }
        let dir = DVec2::new(body.vel.x, body.vel.z);
        let face = if dir.length() > 0.5 {
            dir
        } else if let Job::Answering { at, .. } = hero.job {
            DVec2::new(at.x - body.pos.x, at.z - body.pos.z)
        } else {
            hero.facing
        };
        let q = yaw_quat(face.normalize_or(hero.facing));
        if let Some(mut o) = ent.get_mut::<Orientation>() {
            if o.0 != q {
                o.0 = q;
            }
        }
    }
    world.resource_mut::<Fights>().events.extend(fx);
}

/// Does `inc` want one of them: a fight with villains, left alone for
/// [`ANSWER_AFTER`], with no player hero near enough to be paid for it,
/// nor one on the way to it?
fn wants_help(inc: &Incident, tick: u64, players: &[(NetId, DVec3)]) -> bool {
    let fight = inc.spot.is_some()
        && !matches!(
            inc.kind,
            // The frightened are talked down, and the colossus is the team's.
            IncidentKind::Surfacing | IncidentKind::Colossus
        );
    fight
        && !inc.answered
        && !inc.awaited(tick)
        && inc.calmed.is_none()
        && tick.saturating_sub(inc.started) as f64 * DT >= ANSWER_AFTER
        && players
            .iter()
            .all(|(_, p)| p.distance(inc.at) >= crate::incidents::REWARD)
}

/// Moves `body` one tick toward `goal`: up to cruising height first when far,
/// across, then down. True once there.
fn fly(body: &mut Body, goal: DVec3) -> bool {
    let here = body.pos;
    let flat = DVec2::new(goal.x - here.x, goal.z - here.z);
    let far = flat.length();
    let cruise = goal.y + CRUISE.min(far * 0.4);
    let target = if far > 12.0 {
        DVec3::new(goal.x, cruise, goal.z)
    } else {
        goal
    };
    let mut to = target - here;
    // Climb before crossing: rooftops are in the way low down.
    if far > 12.0 && here.y < cruise - 4.0 {
        let up = (cruise - here.y).min(FLY_SPEED * 0.9);
        let across = flat.normalize_or_zero() * FLY_SPEED * 0.35;
        to = DVec3::new(across.x, up, across.y);
    }
    let step = FLY_SPEED * DT;
    if far <= 12.0 && to.length() <= step {
        body.pos = goal;
        body.vel = DVec3::ZERO;
        body.grounded = false;
        return true;
    }
    let v = to.normalize_or_zero() * FLY_SPEED;
    body.pos += v * DT;
    body.vel = v;
    body.grounded = false;
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flying_climbs_crosses_and_lands() {
        let mut b = Body {
            pos: DVec3::new(0.0, 10.0, 0.0),
            vel: DVec3::ZERO,
            grounded: true,
        };
        let goal = DVec3::new(300.0, 12.0, 0.0);
        let mut top: f64 = 0.0;
        let mut there = false;
        for _ in 0..(60.0 / DT) as usize {
            top = top.max(b.pos.y);
            if fly(&mut b, goal) {
                there = true;
                break;
            }
        }
        assert!(there, "never arrived: {:?}", b.pos);
        assert!(top > goal.y + CRUISE * 0.9, "flew low: {top}");
        assert!(b.pos.distance(goal) < 1e-6);
    }

    #[test]
    fn every_tip_fits_on_a_line() {
        for t in TIPS {
            assert!(t.len() < 110, "{t}");
        }
    }
}
