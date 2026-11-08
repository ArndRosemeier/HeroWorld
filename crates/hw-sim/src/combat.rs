//! Fights, City of Heroes style at its smallest: heroes with health and
//! energy and two powers, the mad scientist's minions in groups, knockback,
//! and nobody dies. Defeated villains are cuffed and lifted away by a police
//! drone; a defeated hero waits for a teammate to revive them, or a medical
//! drone flies them to hospital.
//!
//! Everything here runs on the server in fixed ticks and is a function of
//! the inputs and the seed: fighters are handled in [`NetId`] order and every
//! random choice comes from the world seed, so a test fight ends the same
//! way every time. Clients only show it: the state rides on each entity
//! ([`CombatState`]) and what happened goes out as [`Fx`] events.

use crate::collision::{Body, Obstacles, BODY_HEIGHT};
use crate::{
    step_hero, Control, Hero, Ids, LastChange, Motion, Net, Solids, DT, GROUND_Y, TICK_HZ,
};
use bevy_ecs::prelude::*;
use glam::{DVec2, DVec3};
use hw_core::{NetId, Seed};
use hw_net::{CombatState, EntityKind, Fx, FxKind, InputState, PlayerId, Power, Status};

/// A hero's health and energy at level 1.
pub const HERO_HEALTH: f32 = 100.0;
pub const HERO_ENERGY: f32 = 100.0;
/// No second power while the last one plays out (seconds).
pub const ACTION_LOCK: f64 = 0.5;
/// A power that cannot fire (out of range, recharging) waits this long.
pub const QUEUE_SECONDS: f64 = 1.0;
/// Seconds after the last blow given or taken that a fighter counts as in
/// combat (slow regeneration).
pub const COMBAT_SECONDS: f64 = 6.0;
/// Regeneration per second in and out of combat.
pub const HEALTH_REGEN: (f32, f32) = (1.0, 14.0);
pub const ENERGY_REGEN: (f32, f32) = (6.0, 25.0);
/// A knocked fighter tumbles and lies this many seconds, then takes
/// [`GET_UP_SECONDS`] to stand (it can be hit all along, but not act).
pub const KNOCKED_SECONDS: f64 = 2.2;
pub const GET_UP_SECONDS: f64 = 1.4;
/// Seconds a defeated villain lies cuffed before the police drone comes.
pub const POLICE_DELAY: f64 = 2.0;
/// Seconds a downed hero waits for a teammate before the medical drone
/// comes on its own.
pub const MEDIC_DELAY: f64 = 20.0;
/// Seconds of holding revive next to a downed teammate to get them up, and
/// how close one has to be.
pub const REVIVE_SECONDS: f64 = 3.0;
pub const REVIVE_REACH: f64 = 2.5;
/// A revived hero gets this share of health back.
pub const REVIVE_HEALTH: f32 = 0.5;
/// How high a drone flies above what it carries, how fast, and how far up
/// before it is gone.
pub const DRONE_HANG: f64 = 2.4;
pub const DRONE_SPEED: f64 = 12.0;
pub const DRONE_LIFT_SPEED: f64 = 5.0;
pub const DRONE_LIFT_HEIGHT: f64 = 30.0;
/// Seconds before a beaten group's street corner fills again.
pub const RESPAWN_SECONDS: f64 = 15.0;
/// A group does not come back while a hero stands this close.
pub const RESPAWN_CLEAR: f64 = 25.0;

/// What a power does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PowerSpec {
    pub name: &'static str,
    pub damage: f32,
    pub cost: f32,
    /// Seconds before it can be used again.
    pub recharge: f64,
    /// Metres from the hero to the target.
    pub range: f64,
    /// Every hit knocks the target back (others only when they defeat).
    pub knock: bool,
}

pub fn power_spec(p: Power) -> PowerSpec {
    match p {
        Power::Strike => PowerSpec {
            name: "Strike",
            damage: 20.0,
            cost: 0.0,
            recharge: 0.7,
            range: 3.0,
            knock: false,
        },
        Power::Bolt => PowerSpec {
            name: "Energy Bolt",
            damage: 36.0,
            cost: 10.0,
            recharge: 1.6,
            range: 40.0,
            knock: true,
        },
    }
}

fn slot(p: Power) -> usize {
    match p {
        Power::Strike => 0,
        Power::Bolt => 1,
    }
}

/// The kinds of villains.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EnemyKind {
    /// Melee rusher.
    Brute,
    /// Ranged.
    Technician,
}

/// How a kind of villain fights.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnemySpec {
    pub name: &'static str,
    pub rank: &'static str,
    pub health: f32,
    pub damage: f32,
    /// Seconds between attacks, and of the telegraph before each.
    pub attack_every: f64,
    pub windup: f64,
    /// Reach of the attack.
    pub reach: f64,
    /// Running speed as a share of a person's.
    pub speed: f64,
    /// Notices heroes this close.
    pub aggro: f64,
}

pub fn enemy_spec(k: EnemyKind) -> EnemySpec {
    match k {
        EnemyKind::Brute => EnemySpec {
            name: "Mutant Brute",
            rank: "Minion",
            health: 52.0,
            damage: 7.0,
            attack_every: 2.0,
            windup: 0.5,
            reach: 2.4,
            speed: 0.8,
            aggro: 15.0,
        },
        EnemyKind::Technician => EnemySpec {
            name: "Ray-gun Technician",
            rank: "Minion",
            health: 34.0,
            damage: 6.0,
            attack_every: 2.6,
            windup: 0.7,
            reach: 26.0,
            speed: 0.6,
            aggro: 20.0,
        },
    }
}

impl EnemyKind {
    pub fn entity_kind(self) -> EntityKind {
        match self {
            EnemyKind::Brute => EntityKind::Brute,
            EnemyKind::Technician => EntityKind::Technician,
        }
    }

    pub fn from_entity(k: EntityKind) -> Option<Self> {
        match k {
            EntityKind::Brute => Some(EnemyKind::Brute),
            EntityKind::Technician => Some(EnemyKind::Technician),
            _ => None,
        }
    }
}

/// Health, energy, status and powers of anything that fights.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Fighter {
    pub health: f32,
    pub max_health: f32,
    pub energy: f32,
    pub max_energy: f32,
    pub level: u8,
    pub status: Status,
    /// Tick the status runs out: a knock ends, a drone comes.
    pub until: u64,
    /// Tick of the last blow given or taken.
    pub last_combat: u64,
    /// No power before this tick (the last one still plays out).
    pub busy_until: u64,
    /// Tick each power is ready again.
    pub ready: [u64; 2],
    /// A power waiting to fire, and when it was asked for.
    pub queued: Option<(Power, u64)>,
    /// The hero's picked target, or the villain's foe.
    pub target: Option<NetId>,
    /// Seconds a teammate has held revive on this downed hero.
    pub revive: f64,
    /// A hero's punch or blast this tick is meant for a villain (so it
    /// does not also go into the wall behind).
    pub engaged: bool,
}

impl Fighter {
    pub fn hero() -> Self {
        Self {
            health: HERO_HEALTH,
            max_health: HERO_HEALTH,
            energy: HERO_ENERGY,
            max_energy: HERO_ENERGY,
            level: 1,
            status: Status::Standing,
            until: 0,
            last_combat: 0,
            busy_until: 0,
            ready: [0; 2],
            queued: None,
            target: None,
            revive: 0.0,
            engaged: false,
        }
    }

    fn enemy(kind: EnemyKind, level: u8) -> Self {
        let s = enemy_spec(kind);
        let health = s.health * level_scale(level);
        Self {
            health,
            max_health: health,
            energy: 0.0,
            max_energy: 0.0,
            ..Self::hero()
        }
        .with_level(level)
    }

    fn with_level(mut self, level: u8) -> Self {
        self.level = level;
        self
    }

    /// On its feet and able to act.
    pub fn standing(&self) -> bool {
        self.status == Status::Standing
    }

    /// Can still be hit (standing or tumbling).
    pub fn fighting(&self) -> bool {
        matches!(self.status, Status::Standing | Status::Knocked)
    }

    /// What clients see.
    pub fn view(&self) -> CombatState {
        let bar = |v: f32, max: f32| {
            if max <= 0.0 {
                0
            } else {
                ((v / max).clamp(0.0, 1.0) * 255.0).ceil() as u8
            }
        };
        CombatState {
            health: bar(self.health, self.max_health),
            energy: bar(self.energy, self.max_energy),
            status: self.status,
            level: self.level,
            target: self.target,
        }
    }
}

/// Damage and health grow a tenth per level.
pub fn level_scale(level: u8) -> f32 {
    1.0 + 0.1 * (level.max(1) - 1) as f32
}

/// A villain.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Enemy {
    pub kind: EnemyKind,
    /// The encounter spot it belongs to.
    pub spot: usize,
    /// Where it stands when there is nobody to fight.
    pub home: DVec3,
    /// An attack on its way: (tick it lands, foe).
    pub windup: Option<(u64, NetId)>,
    /// No new attack before this tick.
    pub next_attack: u64,
}

/// A drone lifting someone away.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Drone {
    pub carry: NetId,
    /// Medical (a hero to hospital) or police (a villain to jail).
    pub medical: bool,
    /// Has it picked them up yet, and from how high.
    pub lifting: Option<f64>,
}

/// What clients are told about a fighter: kept apart so regeneration
/// below one step of the bars costs no traffic.
#[derive(Component, Clone, Copy, Debug, PartialEq, Default)]
pub struct CombatView(pub CombatState);

/// A street corner where a group of villains hangs about.
#[derive(Clone, Debug, PartialEq)]
pub struct Spot {
    pub center: DVec3,
    /// The way the group faces (from the centre towards where heroes come).
    pub facing: DVec2,
    /// Groups placed here so far (varies the next one).
    pub groups: u32,
    /// Villains of the current group still here.
    pub members: u32,
    /// The next group comes at this tick.
    pub respawn_at: Option<u64>,
}

/// The fights of a world: encounter spots and the events of the last tick.
#[derive(Resource, Clone, Debug, Default)]
pub struct Fights {
    pub spots: Vec<Spot>,
    /// What happened in the last tick (cleared each tick).
    pub events: Vec<Fx>,
    pub seed: u64,
}

/// One fighter gathered for a tick.
struct Actor {
    e: Entity,
    id: NetId,
    body: Body,
    f: Fighter,
    enemy: Option<Enemy>,
    hero: Option<(PlayerId, InputState)>,
}

impl Actor {
    fn chest(&self) -> DVec3 {
        self.body.pos + DVec3::Y * (BODY_HEIGHT * 0.6)
    }
}

/// The group of villains for a spot: `size` of them, brutes up front and
/// technicians behind, levels 1 and 2 (a level-2 one is a yellow con for a
/// fresh hero).
pub fn roster(seed: Seed, size: u32) -> Vec<(EnemyKind, u8)> {
    let mut rng = seed.rng();
    (0..size)
        .map(|k| {
            // Two in five shoot.
            let kind = if k % 5 == 1 || k % 5 == 3 {
                EnemyKind::Technician
            } else {
                EnemyKind::Brute
            };
            let level = if rng.chance(0.25) { 2 } else { 1 };
            (kind, level)
        })
        .collect()
}

/// Group size for `heroes` players: bigger groups, not tougher minions.
pub fn group_size(heroes: usize) -> u32 {
    (5 + heroes.saturating_sub(1) as u32).min(8)
}

/// Is the straight line from `a` to `b` free of walls?
pub fn clear_line(solids: &Obstacles, a: DVec3, b: DVec3) -> bool {
    let d = b - a;
    let len = d.length();
    let n = (len / 0.5).ceil() as usize;
    (1..n).all(|i| {
        let p = a + d * (i as f64 / n as f64);
        !solids.solid_at(p)
    })
}

fn ticks(seconds: f64) -> u64 {
    (seconds * TICK_HZ as f64).round() as u64
}

/// Spawns a group at `spot` and returns how many.
pub(crate) fn spawn_group(world: &mut World, tick: u64, spot_index: usize, heroes: usize) -> u32 {
    let (seed, spot) = {
        let f = world.resource::<Fights>();
        (f.seed, f.spots[spot_index].clone())
    };
    let solids = world.resource::<Solids>().0.clone();
    let size = group_size(heroes);
    let gseed = Seed(seed)
        .child("fights")
        .child(spot_index as u64)
        .child(spot.groups as u64);
    let side = spot.facing.perp();
    let mut n = 0;
    for (k, (kind, level)) in roster(gseed, size).into_iter().enumerate() {
        // Brutes in a loose front row, technicians a few metres behind.
        let row = if kind == EnemyKind::Brute { 1.5 } else { -3.5 };
        let across = (k as f64 - (size as f64 - 1.0) * 0.5) * 1.6;
        let p = DVec2::new(spot.center.x, spot.center.z) + spot.facing * row + side * across;
        let y = solids.ground_at(p).max(GROUND_Y);
        let home = DVec3::new(p.x, y, p.y);
        let id = world.resource_mut::<Ids>().0.allocate();
        let fighter = Fighter::enemy(kind, level);
        world.spawn((
            Net(id),
            Motion(Body {
                pos: home,
                vel: DVec3::ZERO,
                grounded: true,
            }),
            fighter,
            CombatView(fighter.view()),
            Enemy {
                kind,
                spot: spot_index,
                home,
                windup: None,
                next_attack: tick,
            },
            LastChange(tick),
        ));
        n += 1;
    }
    let mut f = world.resource_mut::<Fights>();
    let s = &mut f.spots[spot_index];
    s.groups += 1;
    s.members = n;
    s.respawn_at = None;
    n
}

/// One tick of every fight in `world` (after heroes moved).
pub(crate) fn step(world: &mut World, tick: u64) {
    world.resource_mut::<Fights>().events.clear();
    let solids = world.resource::<Solids>().0.clone();
    let mut actors: Vec<Actor> = world
        .query::<(
            Entity,
            &Net,
            &Motion,
            &Fighter,
            Option<&Enemy>,
            Option<(&Hero, &Control)>,
        )>()
        .iter(world)
        .map(|(e, n, m, f, en, h)| Actor {
            e,
            id: n.0,
            body: m.0,
            f: *f,
            enemy: en.copied(),
            hero: h.map(|(h, c)| (h.owner, c.0)),
        })
        .collect();
    actors.sort_by_key(|a| a.id);
    let mut drones: Vec<(Entity, NetId, Body, Drone)> = world
        .query::<(Entity, &Net, &Motion, &Drone)>()
        .iter(world)
        .map(|(e, n, m, d)| (e, n.0, m.0, *d))
        .collect();
    drones.sort_by_key(|d| d.1);
    let mut fx = Vec::new();
    let mut spawn_drones: Vec<(NetId, DVec3, bool)> = Vec::new();

    heroes_act(&mut actors, &solids, tick, &mut fx);
    enemies_act(&mut actors, &solids, tick, &mut fx);
    statuses(&mut actors, &drones, tick, &mut fx, &mut spawn_drones);
    regenerate(&mut actors, tick);

    // Drones fly: down to whoever they fetch, then up and away.
    let mut gone: Vec<(Entity, NetId)> = Vec::new();
    let mut back_to_hospital: Vec<NetId> = Vec::new();
    for (de, did, body, d) in drones.iter_mut() {
        let Some(i) = actors.iter().position(|a| a.id == d.carry) else {
            gone.push((*de, *did));
            continue;
        };
        match d.lifting {
            None => {
                let want = actors[i].body.pos + DVec3::Y * DRONE_HANG;
                let to = want - body.pos;
                let step = DRONE_SPEED * DT;
                if to.length() <= step {
                    body.pos = want;
                    body.vel = DVec3::ZERO;
                    d.lifting = Some(want.y);
                    let a = &mut actors[i];
                    a.f.status = Status::Carried;
                    a.body.vel = DVec3::ZERO;
                    fx.push(event(tick, FxKind::Lifted, *did, a.id, a.chest()));
                } else {
                    body.vel = to.normalize() * DRONE_SPEED;
                    body.pos += body.vel * DT;
                }
            }
            Some(from) => {
                body.vel = DVec3::Y * DRONE_LIFT_SPEED;
                body.pos += body.vel * DT;
                let a = &mut actors[i];
                a.body.pos = body.pos - DVec3::Y * DRONE_HANG;
                a.body.vel = body.vel;
                a.body.grounded = false;
                if body.pos.y > from + DRONE_LIFT_HEIGHT {
                    gone.push((*de, *did));
                    if d.medical {
                        back_to_hospital.push(a.id);
                    } else {
                        gone.push((a.e, a.id));
                    }
                }
            }
        }
    }

    // Write everything back; only real changes count.
    for a in &actors {
        if gone.iter().any(|(e, _)| *e == a.e) {
            continue;
        }
        let mut em = world.entity_mut(a.e);
        if let Some(mut m) = em.get_mut::<Motion>() {
            m.set_if_neq(Motion(a.body));
        }
        if let Some(mut f) = em.get_mut::<Fighter>() {
            if *f != a.f {
                *f = a.f;
            }
        }
        if let (Some(en), Some(mut e)) = (a.enemy, em.get_mut::<Enemy>()) {
            if *e != en {
                *e = en;
            }
        }
        let view = a.f.view();
        if let Some(mut v) = em.get_mut::<CombatView>() {
            v.set_if_neq(CombatView(view));
        } else {
            em.insert(CombatView(view));
        }
    }
    for (de, _, body, d) in &drones {
        if gone.iter().any(|(e, _)| e == de) {
            continue;
        }
        let mut em = world.entity_mut(*de);
        if let Some(mut m) = em.get_mut::<Motion>() {
            m.set_if_neq(Motion(*body));
        }
        if let Some(mut x) = em.get_mut::<Drone>() {
            if *x != *d {
                *x = *d;
            }
        }
    }
    for (carry, at, medical) in spawn_drones {
        let id = world.resource_mut::<Ids>().0.allocate();
        // From high above and a little aside, so it is seen coming.
        let start = at + DVec3::new(6.0, 28.0, 4.0);
        world.spawn((
            Net(id),
            Motion(Body {
                pos: start,
                vel: DVec3::ZERO,
                grounded: false,
            }),
            Drone {
                carry,
                medical,
                lifting: None,
            },
            LastChange(tick),
        ));
    }
    // Villains carried off leave their spot; heroes land in hospital.
    for (e, id) in &gone {
        if let Some(a) = actors.iter().find(|a| a.id == *id) {
            if let Some(en) = a.enemy {
                let mut f = world.resource_mut::<Fights>();
                let s = &mut f.spots[en.spot];
                s.members = s.members.saturating_sub(1);
                if s.members == 0 {
                    s.respawn_at = Some(tick + ticks(RESPAWN_SECONDS));
                }
            }
        }
        world.despawn(*e);
    }
    for id in back_to_hospital {
        let hospital = world.resource::<crate::SpawnLine>().hospital(&solids);
        let found = world
            .query::<(Entity, &Net)>()
            .iter(world)
            .find(|(_, n)| n.0 == id)
            .map(|(e, _)| e);
        if let Some(e) = found {
            let mut em = world.entity_mut(e);
            if let Some(mut m) = em.get_mut::<Motion>() {
                m.0 = Body {
                    pos: hospital,
                    vel: DVec3::ZERO,
                    grounded: false,
                };
            }
            if let Some(mut f) = em.get_mut::<Fighter>() {
                let mut fresh = Fighter::hero();
                fresh.level = f.level;
                fresh.last_combat = tick;
                *f = fresh;
                fx.push(event(tick, FxKind::Healed, id, id, hospital.as_vec3()));
            }
        }
    }

    // Empty corners fill again once no hero is near.
    let heroes: Vec<DVec3> = actors
        .iter()
        .filter(|a| a.hero.is_some())
        .map(|a| a.body.pos)
        .collect();
    let due: Vec<usize> = {
        let f = world.resource::<Fights>();
        f.spots
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                s.respawn_at.is_some_and(|t| tick >= t)
                    && heroes.iter().all(|h| h.distance(s.center) > RESPAWN_CLEAR)
            })
            .map(|(i, _)| i)
            .collect()
    };
    for i in due {
        spawn_group(world, tick, i, heroes.len().max(1));
    }
    world.resource_mut::<Fights>().events = fx;
}

fn event(tick: u64, kind: FxKind, from: NetId, to: NetId, at: impl Into<DVec3>) -> Fx {
    let at: DVec3 = at.into();
    Fx {
        tick,
        kind,
        from,
        to,
        at: at.as_vec3().to_array(),
        amount: 0,
        knock: false,
        defeat: false,
    }
}

/// Deals `damage` from actor `from` to actor `to`, knocking it back along
/// `push` if `knock` (or if the blow defeats it). Returns (knocked, defeated).
fn hit(
    actors: &mut [Actor],
    from: usize,
    to: usize,
    damage: f32,
    knock: bool,
    tick: u64,
) -> (bool, bool) {
    let from_id = actors[from].id;
    let from_pos = actors[from].body.pos;
    actors[from].f.last_combat = tick;
    let t = &mut actors[to];
    t.f.health = (t.f.health - damage).max(0.0);
    t.f.last_combat = tick;
    let mut push = DVec2::new(t.body.pos.x - from_pos.x, t.body.pos.z - from_pos.z);
    push = push.normalize_or(DVec2::X);
    let defeated = t.f.health <= 0.0;
    let villain = t.enemy.is_some();
    if villain && defeated {
        t.f.status = Status::Down;
        t.f.until = tick + ticks(POLICE_DELAY);
        t.f.queued = None;
    } else if defeated {
        t.f.status = Status::Down;
        t.f.until = tick + ticks(MEDIC_DELAY);
        t.f.queued = None;
        t.f.revive = 0.0;
    } else if knock && villain {
        t.f.status = Status::Knocked;
        t.f.until = tick + ticks(KNOCKED_SECONDS);
    }
    let knocked = villain && (knock || defeated);
    if knocked {
        let speed = if knock { 9.0 } else { 5.0 };
        t.body.vel = DVec3::new(push.x * speed, 5.0, push.y * speed);
        t.body.grounded = false;
    }
    if let Some(en) = t.enemy.as_mut() {
        en.windup = None;
        // A villain hit turns on whoever hit it.
        if t.f.target.is_none() {
            t.f.target = Some(from_id);
        }
    }
    // The whole group joins in.
    if let Some(spot) = actors[to].enemy.map(|e| e.spot) {
        for a in actors.iter_mut() {
            if a.enemy.is_some_and(|e| e.spot == spot) && a.f.target.is_none() && a.f.fighting() {
                a.f.target = Some(from_id);
            }
        }
    }
    (knocked, defeated)
}

/// Is `a` (by index) a valid target for heroes?
fn hittable(a: &Actor) -> bool {
    a.enemy.is_some() && a.f.fighting()
}

/// The nearest villain within `range` of `from` that can be hit, if any.
fn nearest_enemy(actors: &[Actor], from: DVec3, range: f64) -> Option<usize> {
    actors
        .iter()
        .enumerate()
        .filter(|(_, a)| hittable(a))
        .map(|(i, a)| (i, a.body.pos.distance(from)))
        .filter(|(_, d)| *d <= range)
        .min_by(|x, y| x.1.total_cmp(&y.1))
        .map(|(i, _)| i)
}

fn heroes_act(actors: &mut [Actor], solids: &Obstacles, tick: u64, fx: &mut Vec<Fx>) {
    for i in 0..actors.len() {
        let Some((_, input)) = actors[i].hero else {
            continue;
        };
        actors[i].f.engaged = false;
        // Left mouse punches, Q blasts (held: again and again).
        let asked = if input.blast {
            Some(Power::Bolt)
        } else if input.smash {
            Some(Power::Strike)
        } else {
            None
        };
        if input.target.is_some() || asked.is_some() {
            actors[i].f.target = input.target.or(actors[i].f.target);
        }
        if let Some(t) = actors[i].f.target {
            // A target that is gone or beaten is dropped.
            if !actors.iter().any(|a| a.id == t && hittable(a)) {
                actors[i].f.target = None;
            }
        }
        if let Some(p) = asked {
            actors[i].f.queued = Some((p, tick));
        }
        let Some((power, asked)) = actors[i].f.queued else {
            continue;
        };
        if tick > asked + ticks(QUEUE_SECONDS) || !actors[i].f.standing() {
            actors[i].f.queued = None;
            continue;
        }
        let spec = power_spec(power);
        // The picked target, or else the nearest villain in reach (for a
        // bolt, one near where the hero aims).
        let pos = actors[i].body.pos;
        let aim = DVec3::new(
            input.aim[0] as f64,
            input.aim[1] as f64,
            input.aim[2] as f64,
        )
        .normalize_or(DVec3::ZERO);
        let target = match actors[i].f.target {
            Some(t) => actors.iter().position(|a| a.id == t),
            None if power == Power::Strike => nearest_enemy(actors, pos, spec.range),
            None => {
                let chest = actors[i].chest();
                actors
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| hittable(a))
                    .filter(|(_, a)| {
                        let to = a.chest() - chest;
                        let d = to.length();
                        d <= 25.0 && (aim == DVec3::ZERO || to.dot(aim) > d * 0.85)
                    })
                    .min_by(|x, y| {
                        x.1.body
                            .pos
                            .distance(pos)
                            .total_cmp(&y.1.body.pos.distance(pos))
                    })
                    .map(|(k, _)| k)
            }
        };
        let Some(t) = target else {
            continue;
        };
        let in_reach = actors[t].body.pos.distance(pos) <= spec.range;
        let seen =
            power == Power::Strike || clear_line(solids, actors[i].chest(), actors[t].chest());
        if !in_reach || !seen {
            continue;
        }
        // From here the blow is the villain's, whether it lands now or
        // once the power is ready.
        actors[i].f.engaged = true;
        let me = &actors[i].f;
        if tick < me.busy_until || tick < me.ready[slot(power)] || me.energy < spec.cost {
            continue;
        }
        let me = &mut actors[i].f;
        me.queued = None;
        me.energy -= spec.cost;
        me.busy_until = tick + ticks(ACTION_LOCK);
        me.ready[slot(power)] = tick + ticks(spec.recharge);
        me.target = Some(actors[t].id);
        let damage = spec.damage * level_scale(actors[i].f.level);
        let (knock, defeat) = hit(actors, i, t, damage, spec.knock, tick);
        let mut e = event(
            tick,
            if power == Power::Strike {
                FxKind::Strike
            } else {
                FxKind::Bolt
            },
            actors[i].id,
            actors[t].id,
            actors[t].chest(),
        );
        e.amount = damage.round() as u16;
        e.knock = knock;
        e.defeat = defeat;
        fx.push(e);
    }
}

fn enemies_act(actors: &mut [Actor], solids: &Obstacles, tick: u64, fx: &mut Vec<Fx>) {
    let heroes: Vec<usize> = (0..actors.len())
        .filter(|&i| actors[i].hero.is_some())
        .collect();
    for i in 0..actors.len() {
        let Some(mut en) = actors[i].enemy else {
            continue;
        };
        let spec = enemy_spec(en.kind);
        let mut input = InputState::default();
        let getting_up = tick < actors[i].f.busy_until;
        if actors[i].f.standing() && !getting_up {
            // Who to fight: the foe while it stands and is not led too far
            // from home, else the nearest standing hero it notices.
            let leash = 45.0;
            let foe_ok = |a: &Actor| a.hero.is_some() && a.f.standing();
            let mut foe = actors[i]
                .f
                .target
                .and_then(|t| actors.iter().position(|a| a.id == t))
                .filter(|&j| foe_ok(&actors[j]) && actors[j].body.pos.distance(en.home) < leash);
            if foe.is_none() {
                let me = actors[i].chest();
                foe = heroes
                    .iter()
                    .copied()
                    .filter(|&j| foe_ok(&actors[j]))
                    .map(|j| (j, actors[j].body.pos.distance(actors[i].body.pos)))
                    .filter(|(j, d)| *d < spec.aggro && clear_line(solids, me, actors[*j].chest()))
                    .min_by(|x, y| x.1.total_cmp(&y.1))
                    .map(|(j, _)| j);
                if let Some(j) = foe {
                    // Friends close by come along.
                    let (spot, foe_id) = (en.spot, actors[j].id);
                    for a in actors.iter_mut() {
                        if a.enemy.is_some_and(|e| e.spot == spot)
                            && a.f.target.is_none()
                            && a.f.standing()
                        {
                            a.f.target = Some(foe_id);
                        }
                    }
                }
            }
            actors[i].f.target = foe.map(|j| actors[j].id);
            let here = actors[i].body.pos;
            match foe {
                None => {
                    en.windup = None;
                    // Back to the corner, walking.
                    let to = en.home - here;
                    let flat = DVec2::new(to.x, to.z);
                    if flat.length() > 1.0 {
                        let d = flat.normalize() * 0.3;
                        input.move_x = d.x as f32;
                        input.move_z = d.y as f32;
                    }
                }
                Some(j) => {
                    let there = actors[j].body.pos;
                    let flat = DVec2::new(there.x - here.x, there.z - here.z);
                    let dist = flat.length();
                    let dir = flat.normalize_or(DVec2::X);
                    let seen = clear_line(solids, actors[i].chest(), actors[j].chest());
                    if let Some((at, who)) = en.windup {
                        // The blow lands now, if the foe is still there.
                        if tick >= at {
                            en.windup = None;
                            let target = actors.iter().position(|a| a.id == who);
                            if let Some(t) = target {
                                let d = actors[t].body.pos.distance(actors[i].body.pos);
                                let lands = actors[t].f.standing()
                                    && d <= spec.reach + 0.6
                                    && (en.kind == EnemyKind::Brute || seen);
                                let dmg = spec.damage * level_scale(actors[i].f.level);
                                let kind = match en.kind {
                                    EnemyKind::Brute => FxKind::Punch,
                                    EnemyKind::Technician => FxKind::Ray,
                                };
                                let mut e = event(
                                    tick,
                                    kind,
                                    actors[i].id,
                                    actors[t].id,
                                    actors[t].chest(),
                                );
                                if lands {
                                    let (_, defeat) = hit(actors, i, t, dmg, false, tick);
                                    e.amount = dmg.round() as u16;
                                    e.defeat = defeat;
                                }
                                fx.push(e);
                            }
                        }
                    } else {
                        let (near, far) = match en.kind {
                            EnemyKind::Brute => (0.0, 1.6),
                            EnemyKind::Technician => (7.0, 16.0),
                        };
                        let mut go = DVec2::ZERO;
                        if dist > far || !seen {
                            go = dir * spec.speed;
                        } else if dist < near {
                            go = -dir * spec.speed * 0.8;
                        }
                        let can_attack = dist <= spec.reach && seen && tick >= en.next_attack;
                        if can_attack {
                            let at = tick + ticks(spec.windup);
                            en.windup = Some((at, actors[j].id));
                            en.next_attack = at + ticks(spec.attack_every);
                            let kind = match en.kind {
                                EnemyKind::Brute => FxKind::PunchWindup,
                                EnemyKind::Technician => FxKind::RayWindup,
                            };
                            fx.push(event(
                                tick,
                                kind,
                                actors[i].id,
                                actors[j].id,
                                actors[j].chest(),
                            ));
                            go = DVec2::ZERO;
                        }
                        input.move_x = go.x as f32;
                        input.move_z = go.y as f32;
                    }
                }
            }
            // Keep a little apart from friends, so a group spreads out.
            let mut apart = DVec2::ZERO;
            for (k, a) in actors.iter().enumerate() {
                if k == i || a.enemy.is_none() || !a.f.fighting() {
                    continue;
                }
                let d = DVec2::new(here.x - a.body.pos.x, here.z - a.body.pos.z);
                let l = d.length();
                if l < 1.3 {
                    apart += if l > 1e-6 {
                        d / l * (1.3 - l)
                    } else {
                        DVec2::new(if k < i { 1.0 } else { -1.0 }, 0.0)
                    };
                }
            }
            if en.windup.is_none() || apart.length() > 0.5 {
                input.move_x += apart.x as f32;
                input.move_z += apart.y as f32;
            }
            let len = (input.move_x * input.move_x + input.move_z * input.move_z).sqrt();
            if len > 1.0 {
                input.move_x /= len;
                input.move_z /= len;
            }
        } else {
            en.windup = None;
        }
        actors[i].enemy = Some(en);
        // Walk (or tumble, or lie) with the same movement as people.
        if actors[i].f.status != Status::Carried {
            let mut body = actors[i].body;
            if actors[i].f.status != Status::Standing && body.grounded {
                // On the ground, a tumble ends quickly.
                body.vel.x *= 0.8;
                body.vel.z *= 0.8;
            }
            step_hero(&mut body, input, solids);
            actors[i].body = body;
        }
    }
}

fn statuses(
    actors: &mut [Actor],
    drones: &[(Entity, NetId, Body, Drone)],
    tick: u64,
    fx: &mut Vec<Fx>,
    spawn_drones: &mut Vec<(NetId, DVec3, bool)>,
) {
    let fetched = |id: NetId, spawn: &[(NetId, DVec3, bool)]| {
        drones.iter().any(|d| d.3.carry == id) || spawn.iter().any(|d| d.0 == id)
    };
    for i in 0..actors.len() {
        match actors[i].f.status {
            Status::Knocked if tick >= actors[i].f.until => {
                actors[i].f.status = Status::Standing;
                actors[i].f.busy_until = tick + ticks(GET_UP_SECONDS);
            }
            Status::Down if actors[i].enemy.is_some() => {
                if tick >= actors[i].f.until && !fetched(actors[i].id, spawn_drones) {
                    spawn_drones.push((actors[i].id, actors[i].body.pos, false));
                }
            }
            Status::Down => {
                // A teammate next to them holding revive gets them up.
                let me = actors[i].body.pos;
                let helped = actors.iter().any(|a| {
                    a.hero.is_some_and(|(_, inp)| inp.revive)
                        && a.f.standing()
                        && a.body.pos.distance(me) <= REVIVE_REACH
                });
                let fetching = fetched(actors[i].id, spawn_drones);
                let a = &mut actors[i];
                if helped && !fetching {
                    a.f.revive += DT;
                    if a.f.revive >= REVIVE_SECONDS {
                        a.f.status = Status::Standing;
                        a.f.health = a.f.max_health * REVIVE_HEALTH;
                        a.f.revive = 0.0;
                        a.f.last_combat = tick;
                        fx.push(event(tick, FxKind::Revived, a.id, a.id, a.chest()));
                    }
                } else {
                    a.f.revive = (a.f.revive - DT).max(0.0);
                    let asked = a.hero.is_some_and(|(_, inp)| inp.hospital);
                    if (asked || tick >= a.f.until) && !fetching {
                        spawn_drones.push((a.id, a.body.pos, true));
                    }
                }
            }
            _ => {}
        }
    }
}

fn regenerate(actors: &mut [Actor], tick: u64) {
    for a in actors.iter_mut() {
        if !a.f.fighting() {
            continue;
        }
        let calm = tick.saturating_sub(a.f.last_combat) > ticks(COMBAT_SECONDS);
        let k = if calm { 1 } else { 0 };
        let (h, e) = if a.enemy.is_some() {
            // Villains heal only once nobody fights them.
            (
                if calm && a.f.target.is_none() {
                    10.0
                } else {
                    0.0
                },
                0.0,
            )
        } else {
            (
                [HEALTH_REGEN.0, HEALTH_REGEN.1][k],
                [ENERGY_REGEN.0, ENERGY_REGEN.1][k],
            )
        };
        a.f.health = (a.f.health + h * DT as f32).min(a.f.max_health);
        a.f.energy = (a.f.energy + e * DT as f32).min(a.f.max_energy);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rosters_are_mixed_and_repeatable() {
        let r = roster(Seed(3), 5);
        assert_eq!(r, roster(Seed(3), 5));
        let shooters = r
            .iter()
            .filter(|(k, _)| *k == EnemyKind::Technician)
            .count();
        assert_eq!(shooters, 2);
        assert_eq!(group_size(1), 5);
        assert_eq!(group_size(2), 6);
        assert_eq!(group_size(10), 8);
    }

    #[test]
    fn a_minion_falls_in_one_or_two_hits() {
        for kind in [EnemyKind::Brute, EnemyKind::Technician] {
            let h = enemy_spec(kind).health;
            let bolt = power_spec(Power::Bolt).damage;
            let strike = power_spec(Power::Strike).damage;
            assert!(h <= bolt + strike, "{kind:?}");
            assert!(h > strike, "{kind:?} should take more than a punch");
        }
    }

    #[test]
    fn bars_round_up_so_a_sliver_shows() {
        let mut f = Fighter::hero();
        f.health = 0.1;
        assert_eq!(f.view().health, 1);
        f.health = HERO_HEALTH;
        assert_eq!(f.view().health, 255);
    }
}
