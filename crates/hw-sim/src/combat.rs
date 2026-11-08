//! Fights, City of Heroes style: heroes with health, energy and the powers
//! they bought (see [`crate::progress`]), the mad scientist's minions in
//! groups, knockback, holds and shields, and nobody dies. Defeated villains are cuffed and lifted away by a police
//! drone; a defeated hero waits for a teammate to revive them, or a medical
//! drone flies them to hospital.
//!
//! Everything here runs on the server in fixed ticks and is a function of
//! the inputs and the seed: fighters are handled in [`NetId`] order and every
//! random choice comes from the world seed, so a test fight ends the same
//! way every time. Clients only show it: the state rides on each entity
//! ([`CombatState`]) and what happened goes out as [`Fx`] events.

use crate::collision::{Body, Obstacles, BODY_HEIGHT};
use crate::progress::{is_movement, themes, Affinity};
use crate::robots::{self, Colossus, Mount};
use crate::{
    nav, step_hero, Control, Hero, Ids, LastChange, Motion, Net, Orientation, Prop, Solids, DT,
    GROUND_Y, TICK_HZ,
};
use bevy_ecs::prelude::*;
use glam::{DVec2, DVec3};
use hw_core::{NetId, Seed};
use hw_net::{
    CombatState, EntityKind, Fx, FxKind, InputState, PlayerId, Power, Sheet, Status,
    EFFECT_ARMOURED, EFFECT_CAN_FLY, EFFECT_CARRYING, EFFECT_HELD, EFFECT_SHIELD,
    EFFECT_SUPER_JUMP,
};
use std::collections::BTreeMap;

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
/// An ambient corner's group goes home once no hero is this near (m).
pub const SLEEP: f64 = 420.0;
/// Heroes this close to a defeat share its karma.
pub const REWARD_RADIUS: f64 = 60.0;
/// A force shield takes this share of every blow.
pub const SHIELD_BLOCKS: f32 = 0.5;
/// A lifted thing flies at this speed (m/s across the ground).
pub const THROW_SPEED: f64 = 22.0;
/// How far a hero reaches for something to lift.
pub const LIFT_REACH: f64 = 6.0;
/// Rubble torn up to throw is gone this long after it lands.
pub const RUBBLE_SECONDS: f64 = 20.0;
/// Gravity in Engine's physics (for the arc of a throw).
const PHYSICS_GRAVITY: f64 = 9.81;

/// What a power does at level 1 (damage grows with [`level_scale`]; the
/// rest stays). The theme changes only the look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PowerSpec {
    pub name: &'static str,
    pub damage: f32,
    pub cost: f32,
    /// Seconds before it can be used again.
    pub recharge: f64,
    /// Metres from the hero to the target (0: the hero itself).
    pub range: f64,
    /// Every hit knocks the target back (others only when they defeat).
    pub knock: bool,
    /// Radius it works in round the hero (shockwave, shield), 0 for one target.
    pub area: f64,
    /// Seconds it lasts (a hold, a shield).
    pub lasts: f64,
}

pub fn power_spec(p: Power) -> PowerSpec {
    let none = PowerSpec {
        name: "",
        damage: 0.0,
        cost: 0.0,
        recharge: 0.0,
        range: 0.0,
        knock: false,
        area: 0.0,
        lasts: 0.0,
    };
    match p {
        Power::Strike => PowerSpec {
            name: "Strike",
            damage: 20.0,
            recharge: 0.7,
            range: 3.0,
            ..none
        },
        Power::Bolt => PowerSpec {
            name: "Energy Bolt",
            damage: 36.0,
            cost: 10.0,
            recharge: 1.6,
            range: 40.0,
            knock: true,
            ..none
        },
        Power::SuperJump => PowerSpec {
            name: "Super Jump",
            ..none
        },
        Power::Flight => PowerSpec {
            name: "Flight",
            ..none
        },
        Power::Hold => PowerSpec {
            name: "Binding Hold",
            damage: 12.0,
            cost: 12.0,
            recharge: 6.0,
            range: 30.0,
            lasts: 7.0,
            ..none
        },
        Power::Shield => PowerSpec {
            name: "Force Shield",
            cost: 16.0,
            recharge: 26.0,
            area: 12.0,
            lasts: 18.0,
            ..none
        },
        Power::Lift => PowerSpec {
            name: "Lift and Throw",
            damage: 60.0,
            cost: 12.0,
            recharge: 3.0,
            range: 30.0,
            knock: true,
            ..none
        },
        Power::Shockwave => PowerSpec {
            name: "Shockwave",
            damage: 34.0,
            cost: 26.0,
            recharge: 12.0,
            knock: true,
            area: 8.0,
            ..none
        },
    }
}

fn slot(p: Power) -> usize {
    (p.code() - 1) as usize
}

/// The powers a hero owns and the theme of each, small enough to copy
/// about every tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Kit {
    owned: u16,
    themes: u16,
}

impl Kit {
    pub fn of(sheet: &Sheet) -> Self {
        let mut k = Kit::default();
        for p in &sheet.powers {
            k.owned |= 1 << p.power.code();
            k.themes |= ((p.theme & 1) as u16) << p.power.code();
        }
        k
    }

    /// Every power (villain-free tests, a cheat).
    pub fn everything() -> Self {
        Kit {
            owned: u16::MAX,
            themes: 0,
        }
    }

    pub fn has(self, p: Power) -> bool {
        p == Power::Strike || self.owned & (1 << p.code()) != 0
    }

    pub fn theme(self, p: Power) -> u8 {
        (self.themes >> p.code()) as u8 & 1
    }
}

/// The kinds of villains.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EnemyKind {
    /// Melee rusher.
    Brute,
    /// Ranged.
    Technician,
    /// A lieutenant: the robo-sentry, a ray cannon and a shield projector.
    Sentry,
    /// A boss: the mutant alpha, who leaps onto heroes.
    Alpha,
    /// The walking tank (see [`crate::robots`]).
    Colossus,
    /// A breakable part of a robot (see [`Mount`]).
    WeakPoint,
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
    /// Karma for beating one of the hero's level.
    pub karma: f32,
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
            karma: 7.0,
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
            karma: 6.0,
        },
        EnemyKind::Sentry => EnemySpec {
            name: "Robo-Sentry",
            rank: "Lieutenant",
            health: 150.0,
            damage: 10.0,
            attack_every: 2.4,
            windup: 0.8,
            reach: 26.0,
            speed: 0.5,
            aggro: 24.0,
            karma: 20.0,
        },
        EnemyKind::Alpha => EnemySpec {
            name: "Mutant Alpha",
            rank: "Boss",
            health: 300.0,
            damage: 14.0,
            attack_every: 2.0,
            windup: 0.6,
            reach: 2.8,
            speed: 0.95,
            aggro: 24.0,
            karma: 45.0,
        },
        EnemyKind::Colossus => EnemySpec {
            name: "The Doctor's Colossus",
            rank: "Colossus",
            health: robots::CORE_HEALTH,
            damage: 20.0,
            attack_every: 6.0,
            windup: 1.6,
            reach: 140.0,
            speed: 0.0,
            aggro: 150.0,
            karma: 400.0,
        },
        EnemyKind::WeakPoint => EnemySpec {
            name: "Weak point",
            rank: "Part",
            health: 240.0,
            damage: 0.0,
            attack_every: 1.0,
            windup: 0.0,
            reach: 0.0,
            speed: 0.0,
            aggro: 0.0,
            karma: 25.0,
        },
    }
}

impl EnemyKind {
    pub fn entity_kind(self) -> EntityKind {
        match self {
            EnemyKind::Brute => EntityKind::Brute,
            EnemyKind::Technician => EntityKind::Technician,
            EnemyKind::Sentry => EntityKind::Sentry,
            EnemyKind::Alpha => EntityKind::Alpha,
            EnemyKind::Colossus => EntityKind::Colossus,
            EnemyKind::WeakPoint => EntityKind::WeakPoint,
        }
    }

    /// Machines and bosses: no knockback, no police drone for the machines.
    pub fn heavy(self) -> bool {
        !matches!(self, EnemyKind::Brute | EnemyKind::Technician)
    }

    /// Lifted away by a police drone when beaten (people and sentries).
    pub fn carried_off(self) -> bool {
        !matches!(self, EnemyKind::Colossus | EnemyKind::WeakPoint)
    }

    pub fn from_entity(k: EntityKind) -> Option<Self> {
        match k {
            EntityKind::Brute => Some(EnemyKind::Brute),
            EntityKind::Technician => Some(EnemyKind::Technician),
            EntityKind::Sentry => Some(EnemyKind::Sentry),
            EntityKind::Alpha => Some(EnemyKind::Alpha),
            EntityKind::Colossus => Some(EnemyKind::Colossus),
            EntityKind::WeakPoint => Some(EnemyKind::WeakPoint),
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
    /// Tick each power is ready again (by [`Power::code`] - 1).
    pub ready: [u64; 8],
    /// A power waiting to fire, and when it was asked for.
    pub queued: Option<(Power, u64)>,
    /// The hero's picked target, or the villain's foe.
    pub target: Option<NetId>,
    /// Seconds a teammate has held revive on this downed hero.
    pub revive: f64,
    /// A hero's punch or blast this tick is meant for a villain (so it
    /// does not also go into the wall behind).
    pub engaged: bool,
    /// The powers it has (heroes).
    pub kit: Kit,
    /// Bound by a hold until this tick.
    pub held_until: u64,
    /// Behind a force shield until this tick.
    pub shield_until: u64,
    /// What it holds over its head to throw.
    pub carrying: Option<NetId>,
    /// Effects a robot's state adds ([`EFFECT_ARMOURED`] and the like).
    pub marks: u8,
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
            ready: [0; 8],
            queued: None,
            target: None,
            revive: 0.0,
            engaged: false,
            kit: Kit::default(),
            held_until: 0,
            shield_until: 0,
            carrying: None,
            marks: 0,
        }
    }

    /// A hero of `level` with `kit`, fresh.
    pub fn hero_at(level: u8, kit: Kit) -> Self {
        let mut f = Self::hero();
        f.set_level(level);
        f.health = f.max_health;
        f.kit = kit;
        f
    }

    /// Changes level, keeping the share of health left.
    pub fn set_level(&mut self, level: u8) {
        let share = if self.max_health > 0.0 {
            self.health / self.max_health
        } else {
            1.0
        };
        self.level = level;
        self.max_health = HERO_HEALTH * level_scale(level);
        self.health = self.max_health * share;
    }

    /// Bound by a hold at `tick`.
    pub fn held(&self, tick: u64) -> bool {
        tick < self.held_until
    }

    /// Shielded at `tick`.
    pub fn shielded(&self, tick: u64) -> bool {
        tick < self.shield_until
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

    /// What clients see at `tick`.
    pub fn view(&self, tick: u64) -> CombatState {
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
            effects: (self.held(tick) as u8 * EFFECT_HELD)
                | (self.shielded(tick) as u8 * EFFECT_SHIELD)
                | (self.carrying.is_some() as u8 * EFFECT_CARRYING)
                | (self.kit.has(Power::Flight) as u8 * EFFECT_CAN_FLY)
                | (self.kit.has(Power::SuperJump) as u8 * EFFECT_SUPER_JUMP)
                | self.marks,
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
    /// Up on a roof: it holds its place and shoots down, farther and
    /// sooner, and never walks off the edge.
    pub perch: bool,
    /// A boss's trick (the alpha's leap) is ready again at this tick.
    pub next_trick: u64,
    /// The alpha in the air: (took off, lands, from, to).
    pub leap: Option<(u64, u64, DVec3, DVec3)>,
}

/// A perched villain notices and reaches this much farther.
pub const PERCH_RANGE: f64 = 1.6;

/// A drone lifting someone away.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Drone {
    pub carry: NetId,
    /// Medical (a hero to hospital) or police (a villain to jail).
    pub medical: bool,
    /// Has it picked them up yet, and from how high.
    pub lifting: Option<f64>,
}

/// Something a hero lifted: it rides over their head until thrown.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Carried(pub NetId);

/// A chunk torn out of the street to throw: gone a while after it lands.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rubble {
    pub gone_at: Option<u64>,
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
    /// Levels its villains stand above the team's (the mad scientist's
    /// quarter is tougher).
    pub extra: u8,
    /// An ambient corner: its group only stands there while a hero is
    /// within this many metres, and leaves (to come back fresh) once
    /// nobody is within [`SLEEP`].
    pub wake: Option<f64>,
    /// An incident's group: this many (else by the number of heroes), of
    /// this kind only (else brutes and technicians), and never refilled.
    pub size: Option<u32>,
    pub only: Option<EnemyKind>,
    pub once: bool,
    /// On a roof (`center` on top of it): its group stands up there.
    pub perch: bool,
    /// A lieutenant or boss in front of the group.
    pub lead: Option<EnemyKind>,
}

impl Spot {
    /// A street corner whose group stands there at once and comes back.
    pub fn corner(center: DVec3, facing: DVec2) -> Self {
        Self {
            center,
            facing: facing.normalize_or(DVec2::X),
            groups: 0,
            members: 0,
            respawn_at: None,
            extra: 0,
            wake: None,
            size: None,
            only: None,
            once: false,
            perch: false,
            lead: None,
        }
    }
}

/// Karma (and leaning) a hero earned this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reward {
    pub hero: NetId,
    pub karma: u32,
    /// The deed's affinity (half the karma leans this way).
    pub deed: Option<Affinity>,
    /// Extra leaning for deeds that pay no karma (a revive).
    pub lean: u32,
}

/// Something thrown, on its way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Throw {
    /// The tick it lands.
    pub land: u64,
    pub from: NetId,
    pub to: NetId,
    pub damage: f32,
    pub theme: u8,
}

/// A villain's way round walls: where it is going and the corners to it.
#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pub made: u64,
    pub goal: DVec3,
    pub points: Vec<DVec3>,
}

/// The fights of a world: encounter spots and the events of the last tick.
#[derive(Resource, Clone, Debug)]
pub struct Fights {
    pub spots: Vec<Spot>,
    /// What happened in the last tick (cleared each tick).
    pub events: Vec<Fx>,
    pub seed: u64,
    /// The level heroes fight at (the host's hero's): new groups come at it.
    pub team_level: u8,
    /// Karma earned in the last tick (the simulation pays it out).
    pub rewards: Vec<Reward>,
    /// Throws in the air.
    pub throws: Vec<Throw>,
    /// Villains' paths round walls.
    pub paths: BTreeMap<NetId, Path>,
    /// Blows at the city this tick (the colossus's feet and shells):
    /// (where, radius, power); the simulation breaks buildings with them.
    pub city: Vec<(DVec3, f32, f32)>,
}

impl Default for Fights {
    fn default() -> Self {
        Self {
            spots: Vec::new(),
            events: Vec::new(),
            seed: 0,
            team_level: 1,
            rewards: Vec::new(),
            throws: Vec::new(),
            paths: BTreeMap::new(),
            city: Vec::new(),
        }
    }
}

/// One fighter gathered for a tick.
pub(crate) struct Actor {
    pub(crate) e: Entity,
    pub(crate) id: NetId,
    pub(crate) body: Body,
    pub(crate) f: Fighter,
    pub(crate) enemy: Option<Enemy>,
    pub(crate) hero: Option<(PlayerId, InputState)>,
    /// Who beat it this tick, and the affinity of the power that did.
    beaten_by: Option<(NetId, Option<Affinity>)>,
    /// A weak point's robot.
    pub(crate) mount: Option<Mount>,
    /// The walking tank's own state.
    pub(crate) colossus: Option<Colossus>,
}

impl Actor {
    pub(crate) fn chest(&self) -> DVec3 {
        self.body.pos + DVec3::Y * (BODY_HEIGHT * 0.6)
    }
}

/// The group of villains for a spot: `size` of them, brutes up front and
/// technicians behind, at `level` and now and then one above (a yellow con).
pub fn roster(seed: Seed, size: u32, level: u8) -> Vec<(EnemyKind, u8)> {
    let mut rng = seed.rng();
    (0..size)
        .map(|k| {
            // Two in five shoot.
            let kind = if k % 5 == 1 || k % 5 == 3 {
                EnemyKind::Technician
            } else {
                EnemyKind::Brute
            };
            let level = if rng.chance(0.25) {
                (level + 1).min(crate::progress::MAX_LEVEL)
            } else {
                level
            };
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

pub(crate) fn ticks(seconds: f64) -> u64 {
    (seconds * TICK_HZ as f64).round() as u64
}

/// Spawns a group at `spot` and returns how many.
pub(crate) fn spawn_group(world: &mut World, tick: u64, spot_index: usize, heroes: usize) -> u32 {
    let (seed, spot) = {
        let f = world.resource::<Fights>();
        (f.seed, f.spots[spot_index].clone())
    };
    let solids = world.resource::<Solids>().0.clone();
    let size = spot
        .size
        .unwrap_or(group_size(heroes) + spot.extra.min(1) as u32);
    let level = world
        .resource::<Fights>()
        .team_level
        .saturating_add(spot.extra)
        .min(crate::progress::MAX_LEVEL);
    let gseed = Seed(seed)
        .child("fights")
        .child(spot_index as u64)
        .child(spot.groups as u64);
    let side = spot.facing.perp();
    let mut n = 0;
    let mut group: Vec<(EnemyKind, u8)> = roster(gseed, size, level)
        .into_iter()
        .map(|(kind, level)| (spot.only.unwrap_or(kind), level))
        .collect();
    // A lieutenant or boss takes the place of the first in line.
    if let (Some(lead), Some(first)) = (spot.lead, group.first_mut()) {
        *first = (
            lead,
            level.saturating_add(1).min(crate::progress::MAX_LEVEL),
        );
    }
    let count = group.len();
    for (k, (kind, level)) in group.into_iter().enumerate() {
        // Brutes in a loose front row, shooters a few metres behind.
        let row = match kind {
            EnemyKind::Technician | EnemyKind::Sentry => -3.5,
            EnemyKind::Colossus => 0.0,
            _ => 1.5,
        };
        let across = if kind == EnemyKind::Colossus {
            0.0
        } else {
            (k as f64 - (count as f64 - 1.0) * 0.5) * 1.6
        };
        let p = DVec2::new(spot.center.x, spot.center.z) + spot.facing * row + side * across;
        let y = if spot.perch {
            solids.floor_under(p, spot.center.y + 1.0)
        } else {
            solids.ground_at(p).max(GROUND_Y)
        };
        let home = DVec3::new(p.x, y, p.y);
        let id = spawn_enemy(world, tick, kind, level, spot_index, home, heroes);
        if kind == EnemyKind::Colossus {
            let yaw = spot.facing.x.atan2(spot.facing.y);
            robots::spawn_colossus(world, tick, id, spot_index, home, yaw, level, heroes);
        } else if kind == EnemyKind::Sentry {
            robots::spawn_parts(
                world,
                tick,
                id,
                spot_index,
                home,
                level,
                heroes,
                &[robots::DISH],
            );
        }
        n += 1;
    }
    let mut f = world.resource_mut::<Fights>();
    let s = &mut f.spots[spot_index];
    s.groups += 1;
    s.members = n;
    s.respawn_at = None;
    n
}

/// Spawns one villain of `kind` at `home` for `spot`; returns its ID.
pub(crate) fn spawn_enemy(
    world: &mut World,
    tick: u64,
    kind: EnemyKind,
    level: u8,
    spot: usize,
    home: DVec3,
    heroes: usize,
) -> NetId {
    let id = world.resource_mut::<Ids>().0.allocate();
    let mut fighter = Fighter::enemy(kind, level);
    // The colossus and its parts take a team: tougher for more heroes.
    if matches!(kind, EnemyKind::Colossus | EnemyKind::WeakPoint) {
        let k = 1.0 + 0.6 * heroes.saturating_sub(1) as f32;
        fighter.max_health *= k;
        fighter.health = fighter.max_health;
    }
    world.spawn((
        Net(id),
        Motion(Body {
            pos: home,
            vel: DVec3::ZERO,
            grounded: true,
        }),
        fighter,
        CombatView(fighter.view(tick)),
        Enemy {
            kind,
            spot,
            home,
            windup: None,
            next_attack: tick,
            perch: world
                .resource::<Fights>()
                .spots
                .get(spot)
                .is_some_and(|s| s.perch),
            next_trick: tick + ticks(4.0),
            leap: None,
        },
        LastChange(tick),
    ));
    id
}

/// Height over a hero's feet where a lifted thing rides.
pub const CARRY_HEIGHT: f64 = 2.6;

/// What one tick of fighting produced, beyond the fighters themselves.
#[derive(Default)]
struct Out {
    fx: Vec<Fx>,
    /// Heroes (by actor index) reaching for something to lift.
    lifts: Vec<usize>,
    /// Lifted things let go: (what, velocity).
    tosses: Vec<(NetId, DVec3)>,
    /// New throws in the air.
    throws: Vec<Throw>,
    rewards: Vec<Reward>,
}

/// One tick of every fight in `world` (after heroes moved).
pub(crate) fn step(world: &mut World, tick: u64) {
    world.resource_mut::<Fights>().events.clear();
    world.resource_mut::<Fights>().rewards.clear();
    let solids = world.resource::<Solids>().0.clone();
    let mut actors: Vec<Actor> = world
        .query::<(
            Entity,
            &Net,
            &Motion,
            &Fighter,
            Option<&Enemy>,
            Option<(&Hero, &Control)>,
            (Option<&Mount>, Option<&Colossus>),
        )>()
        .iter(world)
        .map(|(e, n, m, f, en, h, (mount, colossus))| Actor {
            e,
            id: n.0,
            body: m.0,
            f: *f,
            enemy: en.copied(),
            hero: h.map(|(h, c)| (h.owner, c.0)),
            beaten_by: None,
            mount: mount.copied(),
            colossus: colossus.copied(),
        })
        .collect();
    actors.sort_by_key(|a| a.id);
    let mut drones: Vec<(Entity, NetId, Body, Drone)> = world
        .query::<(Entity, &Net, &Motion, &Drone)>()
        .iter(world)
        .map(|(e, n, m, d)| (e, n.0, m.0, *d))
        .collect();
    drones.sort_by_key(|d| d.1);
    let mut out = Out::default();
    let mut spawn_drones: Vec<(NetId, DVec3, bool)> = Vec::new();
    let mut paths = std::mem::take(&mut world.resource_mut::<Fights>().paths);
    let mut throws = std::mem::take(&mut world.resource_mut::<Fights>().throws);

    heroes_act(&mut actors, &solids, tick, &mut out);
    land_throws(&mut actors, &mut throws, tick, &mut out);
    throws.append(&mut out.throws);
    enemies_act(&mut actors, &solids, tick, &mut out.fx, &mut paths);
    let mut city = Vec::new();
    robots::act(&mut actors, &solids, tick, &mut out.fx, &mut city);
    statuses(&mut actors, &drones, tick, &mut out, &mut spawn_drones);
    regenerate(&mut actors, tick);
    pay_out(&actors, &mut out);
    // Villains gone (carried off) need no path.
    paths.retain(|id, _| actors.iter().any(|a| a.id == *id && a.enemy.is_some()));
    {
        let mut f = world.resource_mut::<Fights>();
        f.paths = paths;
        f.throws = throws;
    }
    let mut fx = std::mem::take(&mut out.fx);

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

    // Heroes reach for something to lift: the nearest loose thing, or a
    // chunk torn out of the street.
    for i in std::mem::take(&mut out.lifts) {
        let hero = &actors[i];
        let at = hero.body.pos;
        let mut near: Vec<(NetId, Entity, f64)> = world
            .query_filtered::<(Entity, &Net, &Motion), (With<Prop>, Without<Carried>)>()
            .iter(world)
            .map(|(e, n, m)| (n.0, e, m.0.pos.distance(at)))
            .filter(|x| x.2 <= LIFT_REACH)
            .collect();
        near.sort_by(|a, b| a.2.total_cmp(&b.2).then(a.0.cmp(&b.0)));
        let id = match near.first() {
            Some((id, e, _)) => {
                world.entity_mut(*e).insert(Carried(hero.id));
                *id
            }
            None => {
                let id = world.resource_mut::<Ids>().0.allocate();
                world.spawn((
                    Net(id),
                    Motion(Body {
                        pos: at + DVec3::Y * CARRY_HEIGHT,
                        vel: DVec3::ZERO,
                        grounded: false,
                    }),
                    Orientation(hw_net::protocol::NO_ROTATION),
                    Prop,
                    Rubble { gone_at: None },
                    Carried(hero.id),
                    LastChange(tick),
                ));
                id
            }
        };
        let theme = hero.f.kit.theme(Power::Lift);
        let mut e = event(
            tick,
            FxKind::Lift,
            hero.id,
            id,
            at + DVec3::Y * CARRY_HEIGHT,
        );
        e.theme = theme;
        fx.push(e);
        actors[i].f.carrying = Some(id);
    }
    // Lifted things ride over their heroes' heads, and fall if they fall.
    let mut riding: BTreeMap<NetId, Body> = BTreeMap::new();
    let mut dropped: Vec<NetId> = Vec::new();
    for a in actors.iter_mut() {
        let Some(c) = a.f.carrying else {
            continue;
        };
        if a.f.standing() {
            riding.insert(
                c,
                Body {
                    pos: a.body.pos + DVec3::Y * CARRY_HEIGHT,
                    vel: a.body.vel,
                    grounded: false,
                },
            );
        } else {
            a.f.carrying = None;
            dropped.push(c);
        }
    }
    let tosses = std::mem::take(&mut out.tosses);
    let mut lost: Vec<NetId> = Vec::new();
    for (e, n, mut m, rubble) in world
        .query_filtered::<(Entity, &Net, &mut Motion, Option<&mut Rubble>), With<Carried>>()
        .iter_mut(world)
        .map(|(e, n, m, r)| (e, n.0, m, r))
        .collect::<Vec<_>>()
    {
        let _ = e;
        if let Some(b) = riding.get(&n) {
            m.set_if_neq(Motion(*b));
        } else if let Some((_, v)) = tosses.iter().find(|t| t.0 == n) {
            m.0.vel = *v;
            if let Some(mut r) = rubble {
                r.gone_at = Some(tick + ticks(RUBBLE_SECONDS));
            }
            lost.push(n);
        } else {
            if let Some(mut r) = rubble {
                r.gone_at = Some(tick + ticks(RUBBLE_SECONDS));
            }
            lost.push(n);
        }
    }
    let unhook: Vec<Entity> = world
        .query_filtered::<(Entity, &Net), With<Carried>>()
        .iter(world)
        .filter(|(_, n)| lost.contains(&n.0) || dropped.contains(&n.0))
        .map(|(e, _)| e)
        .collect();
    for e in unhook {
        world.entity_mut(e).remove::<Carried>();
    }
    // A hero whose lifted thing is gone holds nothing.
    let props: Vec<NetId> = world
        .query_filtered::<&Net, With<Carried>>()
        .iter(world)
        .map(|n| n.0)
        .collect();
    for a in actors.iter_mut() {
        if a.f.carrying.is_some_and(|c| !props.contains(&c)) {
            a.f.carrying = None;
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
        if let (Some(c), Some(mut x)) = (a.colossus, em.get_mut::<Colossus>()) {
            if *x != c {
                *x = c;
            }
        }
        if let Some(c) = a.colossus {
            let rot = robots::yaw_rotation(c.yaw);
            if let Some(mut o) = em.get_mut::<Orientation>() {
                o.set_if_neq(Orientation(rot));
            }
        }
        let view = a.f.view(tick);
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
                if s.members == 0 && !s.once {
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
                let mut fresh = Fighter::hero_at(f.level, f.kit);
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
                    && s.wake
                        .is_none_or(|r| heroes.iter().any(|h| h.distance(s.center) < r))
            })
            .map(|(i, _)| i)
            .collect()
    };
    for i in due {
        spawn_group(world, tick, i, heroes.len().max(1));
    }
    // Ambient corners nobody is near any more: their villains go home, and
    // a fresh group stands there when a hero comes back.
    let idle: Vec<usize> = {
        let f = world.resource::<Fights>();
        f.spots
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                s.wake.is_some()
                    && s.members > 0
                    && heroes.iter().all(|h| h.distance(s.center) > SLEEP)
            })
            .map(|(i, _)| i)
            .collect()
    };
    if !idle.is_empty() {
        let mut home: Vec<(NetId, Entity)> = world
            .query::<(Entity, &Net, &Enemy)>()
            .iter(world)
            .filter(|(_, _, en)| idle.contains(&en.spot))
            .map(|(e, n, _)| (n.0, e))
            .collect();
        home.sort_by_key(|h| h.0);
        for (id, e) in home {
            world.resource_mut::<Fights>().paths.remove(&id);
            world.despawn(e);
        }
        let mut f = world.resource_mut::<Fights>();
        for i in idle {
            f.spots[i].members = 0;
            f.spots[i].respawn_at = Some(tick);
        }
    }
    robots::tidy(world, tick);
    let mut f = world.resource_mut::<Fights>();
    f.events = fx;
    f.rewards = out.rewards;
    f.city = city;
}

pub(crate) fn event(tick: u64, kind: FxKind, from: NetId, to: NetId, at: impl Into<DVec3>) -> Fx {
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
        theme: 0,
    }
}

/// Deals `damage` from actor `from` to actor `to` (a shield takes its
/// share), knocking it back if `knock` (or if the blow defeats it). The
/// power's `affinity` is remembered for the karma of a defeat. Returns
/// (knocked, defeated, damage dealt).
pub(crate) fn hit(
    actors: &mut [Actor],
    from: usize,
    to: usize,
    damage: f32,
    knock: bool,
    tick: u64,
    affinity: Option<Affinity>,
) -> (bool, bool, f32) {
    let from_id = actors[from].id;
    let from_pos = actors[from].body.pos;
    actors[from].f.last_combat = tick;
    let t = &mut actors[to];
    let mut dealt = if t.f.shielded(tick) {
        damage * (1.0 - SHIELD_BLOCKS)
    } else {
        damage
    };
    // Shut armour: blows glance off.
    if t.f.marks & EFFECT_ARMOURED != 0 {
        dealt *= robots::ARMOUR_LETS_THROUGH;
    }
    let heavy = t.enemy.is_some_and(|e| e.kind.heavy());
    let knock = knock && !heavy;
    t.f.health = (t.f.health - dealt).max(0.0);
    t.f.last_combat = tick;
    let mut push = DVec2::new(t.body.pos.x - from_pos.x, t.body.pos.z - from_pos.z);
    push = push.normalize_or(DVec2::X);
    let defeated = t.f.health <= 0.0;
    let villain = t.enemy.is_some();
    if villain && defeated {
        t.f.status = Status::Down;
        t.f.until = tick + ticks(POLICE_DELAY);
        if heavy {
            t.body.vel = DVec3::ZERO;
        }
        t.f.queued = None;
        t.f.held_until = 0;
        t.beaten_by = Some((from_id, affinity));
    } else if defeated {
        t.f.status = Status::Down;
        t.f.until = tick + ticks(MEDIC_DELAY);
        t.f.queued = None;
        t.f.revive = 0.0;
    } else if knock && villain {
        t.f.status = Status::Knocked;
        t.f.until = tick + ticks(KNOCKED_SECONDS);
    }
    let knocked = villain && (knock || defeated) && !heavy;
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
    (knocked, defeated, dealt)
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

/// The affinity a hero's power leans toward, in the theme they chose.
fn power_affinity(kit: Kit, p: Power) -> Option<Affinity> {
    themes(p)[kit.theme(p) as usize].affinity
}

fn heroes_act(actors: &mut [Actor], solids: &Obstacles, tick: u64, out: &mut Out) {
    for i in 0..actors.len() {
        let Some((_, input)) = actors[i].hero else {
            continue;
        };
        actors[i].f.engaged = false;
        let kit = actors[i].f.kit;
        // Left mouse punches, Q blasts (held: again and again), keys 3 to 6
        // the other powers.
        let asked = if input.blast && kit.has(Power::Bolt) {
            Some(Power::Bolt)
        } else if input.smash {
            Some(Power::Strike)
        } else {
            Power::from_code(input.power).filter(|p| kit.has(*p) && !is_movement(*p))
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
        let theme = kit.theme(power);
        let throwing = power == Power::Lift && actors[i].f.carrying.is_some();
        let aimed = matches!(power, Power::Strike | Power::Bolt | Power::Hold) || throwing;
        // The picked target, or else the nearest villain in reach (for a
        // ranged power, one near where the hero aims).
        let pos = actors[i].body.pos;
        let aim = DVec3::new(
            input.aim[0] as f64,
            input.aim[1] as f64,
            input.aim[2] as f64,
        )
        .normalize_or(DVec3::ZERO);
        let mut target = None;
        if aimed {
            target = match actors[i].f.target {
                Some(t) => actors.iter().position(|a| a.id == t),
                None if power == Power::Strike => nearest_enemy(actors, pos, spec.range),
                None => {
                    let chest = actors[i].chest();
                    let reach = spec.range.min(25.0);
                    actors
                        .iter()
                        .enumerate()
                        .filter(|(_, a)| hittable(a))
                        .filter(|(_, a)| {
                            let to = a.chest() - chest;
                            let d = to.length();
                            d <= reach && (aim == DVec3::ZERO || to.dot(aim) > d * 0.85)
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
            if let Some(t) = target {
                let in_reach = actors[t].body.pos.distance(pos) <= spec.range;
                let seen = power == Power::Strike
                    || clear_line(solids, actors[i].chest(), actors[t].chest());
                if !in_reach || !seen {
                    target = None;
                }
            }
            if target.is_none() && !throwing {
                continue;
            }
            // From here the blow is the villain's, whether it lands now or
            // once the power is ready.
            actors[i].f.engaged = target.is_some();
        }
        if power == Power::Shockwave && nearest_enemy(actors, pos, spec.area).is_some() {
            actors[i].f.engaged = true;
        }
        let me = &actors[i].f;
        // Lifting paid for the throw; a lift waits for the last throw's recharge.
        let cost = if throwing { 0.0 } else { spec.cost };
        let ready = throwing || tick >= me.ready[slot(power)];
        if tick < me.busy_until || !ready || me.energy < cost {
            continue;
        }
        let me_id = actors[i].id;
        let me = &mut actors[i].f;
        me.queued = None;
        me.energy -= cost;
        me.busy_until = tick + ticks(ACTION_LOCK);
        if power != Power::Lift || throwing {
            me.ready[slot(power)] = tick + ticks(spec.recharge);
        }
        let damage = spec.damage * level_scale(me.level);
        let affinity = power_affinity(kit, power);
        match power {
            Power::Strike | Power::Bolt | Power::Hold => {
                let Some(t) = target else {
                    continue;
                };
                actors[i].f.target = Some(actors[t].id);
                let (knock, defeat, dealt) = hit(actors, i, t, damage, spec.knock, tick, affinity);
                if power == Power::Hold && !defeat {
                    // Tougher villains shake a hold off sooner.
                    let con = actors[t].f.level as f64 - actors[i].f.level as f64;
                    let secs = spec.lasts * (1.0 - 0.15 * con).clamp(0.4, 1.3);
                    actors[t].f.held_until = tick + ticks(secs);
                }
                let kind = match power {
                    Power::Strike => FxKind::Strike,
                    Power::Bolt => FxKind::Bolt,
                    _ => FxKind::Hold,
                };
                let mut e = event(tick, kind, me_id, actors[t].id, actors[t].chest());
                e.amount = dealt.round() as u16;
                e.knock = knock;
                e.defeat = defeat;
                e.theme = theme;
                out.fx.push(e);
            }
            Power::Shield => {
                let until = tick + ticks(spec.lasts);
                for (k, a) in actors.iter_mut().enumerate() {
                    if a.hero.is_none() || !a.f.fighting() || a.body.pos.distance(pos) > spec.area {
                        continue;
                    }
                    a.f.shield_until = until;
                    let mut e = event(tick, FxKind::Shield, me_id, a.id, a.chest());
                    e.theme = theme;
                    out.fx.push(e);
                    if k != i {
                        out.rewards.push(Reward {
                            hero: me_id,
                            karma: 0,
                            deed: Some(Affinity::Guardian),
                            lean: 5,
                        });
                    }
                }
            }
            Power::Shockwave => {
                let chest = actors[i].chest();
                let mut caught = 0;
                for k in 0..actors.len() {
                    if !hittable(&actors[k])
                        || actors[k].body.pos.distance(pos) > spec.area
                        || !clear_line(solids, chest, actors[k].chest())
                    {
                        continue;
                    }
                    let (knock, defeat, dealt) = hit(actors, i, k, damage, true, tick, affinity);
                    let mut e = event(
                        tick,
                        FxKind::Shockwave,
                        me_id,
                        actors[k].id,
                        actors[k].chest(),
                    );
                    e.amount = dealt.round() as u16;
                    e.knock = knock;
                    e.defeat = defeat;
                    e.theme = theme;
                    out.fx.push(e);
                    caught += 1;
                }
                actors[i].f.last_combat = tick;
                let mut e = event(tick, FxKind::Shockwave, me_id, me_id, pos);
                e.amount = caught;
                e.theme = theme;
                out.fx.push(e);
            }
            Power::Lift if !throwing => out.lifts.push(i),
            Power::Lift => {
                let Some(prop) = actors[i].f.carrying.take() else {
                    continue;
                };
                let from = pos + DVec3::Y * CARRY_HEIGHT;
                let ahead = if aim == DVec3::ZERO {
                    let v = DVec3::new(actors[i].body.vel.x, 0.0, actors[i].body.vel.z);
                    v.normalize_or(DVec3::X)
                } else {
                    aim
                };
                let (to, tid) = match target {
                    Some(t) => (actors[t].chest(), Some(actors[t].id)),
                    None => (from + ahead * 15.0 - DVec3::Y * 2.0, None),
                };
                let flat = DVec2::new(to.x - from.x, to.z - from.z);
                let time = (flat.length() / THROW_SPEED).max(0.15);
                let up = (to.y - from.y + 0.5 * PHYSICS_GRAVITY * time * time) / time;
                out.tosses
                    .push((prop, DVec3::new(flat.x / time, up, flat.y / time)));
                if let Some(t) = tid {
                    actors[i].f.target = Some(t);
                    out.throws.push(Throw {
                        land: tick + ticks(time).max(1),
                        from: me_id,
                        to: t,
                        damage,
                        theme,
                    });
                }
                actors[i].f.last_combat = tick;
                let mut e = event(tick, FxKind::Throw, me_id, tid.unwrap_or(me_id), from);
                e.theme = theme;
                out.fx.push(e);
            }
            Power::SuperJump | Power::Flight => {}
        }
    }
}

/// Thrown things that arrive this tick hit whoever they were thrown at (if
/// still there to be hit).
fn land_throws(actors: &mut [Actor], throws: &mut Vec<Throw>, tick: u64, out: &mut Out) {
    let due: Vec<Throw> = throws.iter().copied().filter(|t| t.land <= tick).collect();
    throws.retain(|t| t.land > tick);
    for th in due {
        let from = actors.iter().position(|a| a.id == th.from);
        let to = actors.iter().position(|a| a.id == th.to);
        let (Some(i), Some(t)) = (from, to) else {
            continue;
        };
        if !hittable(&actors[t]) {
            continue;
        }
        let affinity = themes(Power::Lift)[th.theme as usize & 1].affinity;
        let (knock, defeat, dealt) = hit(actors, i, t, th.damage, true, tick, affinity);
        let mut e = event(tick, FxKind::ThrowHit, th.from, th.to, actors[t].chest());
        e.amount = dealt.round() as u16;
        e.knock = knock;
        e.defeat = defeat;
        e.theme = th.theme;
        out.fx.push(e);
    }
}

/// Karma for every villain beaten this tick, to each hero near enough to
/// have helped: tougher ones pay more. The one who beat it leans toward
/// their power's affinity, everyone toward Justice.
fn pay_out(actors: &[Actor], out: &mut Out) {
    for v in actors {
        let (Some((by, affinity)), Some(en)) = (v.beaten_by, v.enemy) else {
            continue;
        };
        let base = enemy_spec(en.kind).karma;
        for h in actors.iter().filter(|a| a.hero.is_some()) {
            if h.body.pos.distance(v.body.pos) > REWARD_RADIUS {
                continue;
            }
            let deed = if h.id == by {
                affinity.or(Some(Affinity::Justice))
            } else {
                Some(Affinity::Justice)
            };
            out.rewards.push(Reward {
                hero: h.id,
                karma: crate::progress::defeat_karma(base, v.f.level, h.f.level),
                deed,
                lean: 0,
            });
        }
    }
}

/// Which way a villain at `here` walks to reach `goal`: straight when the
/// way is clear, else along a path round the walls (found again now and
/// then, as the goal moves).
fn heading(
    paths: &mut BTreeMap<NetId, Path>,
    solids: &Obstacles,
    id: NetId,
    tick: u64,
    here: DVec3,
    goal: DVec3,
) -> DVec2 {
    let flat = |a: DVec3, b: DVec3| DVec2::new(b.x - a.x, b.z - a.z);
    let direct = flat(here, goal).normalize_or_zero();
    if nav::walkable_line(solids, here, goal) {
        paths.remove(&id);
        return direct;
    }
    let stale = paths
        .get(&id)
        .is_none_or(|p| tick >= p.made + ticks(1.5) || p.goal.distance(goal) > 3.0);
    if stale {
        match nav::find_path(solids, here, goal) {
            Some(points) => {
                paths.insert(
                    id,
                    Path {
                        made: tick,
                        goal,
                        points,
                    },
                );
            }
            None => {
                paths.remove(&id);
                return direct;
            }
        }
    }
    let Some(p) = paths.get_mut(&id) else {
        return direct;
    };
    while p.points.len() > 1 && flat(here, p.points[0]).length() < 0.8 {
        p.points.remove(0);
    }
    flat(here, p.points[0]).normalize_or(direct)
}

fn enemies_act(
    actors: &mut [Actor],
    solids: &Obstacles,
    tick: u64,
    fx: &mut Vec<Fx>,
    paths: &mut BTreeMap<NetId, Path>,
) {
    let heroes: Vec<usize> = (0..actors.len())
        .filter(|&i| actors[i].hero.is_some())
        .collect();
    for i in 0..actors.len() {
        let Some(mut en) = actors[i].enemy else {
            continue;
        };
        // Machines that are not people-sized move in `robots`.
        if matches!(en.kind, EnemyKind::Colossus | EnemyKind::WeakPoint) {
            continue;
        }
        if en.leap.is_some() {
            leap(actors, i, &mut en, tick, fx);
            actors[i].enemy = Some(en);
            continue;
        }
        let mut spec = enemy_spec(en.kind);
        if en.perch {
            spec.aggro *= PERCH_RANGE;
            spec.reach *= PERCH_RANGE;
        }
        let ranged = matches!(en.kind, EnemyKind::Technician | EnemyKind::Sentry);
        let mut input = InputState::default();
        let getting_up = tick < actors[i].f.busy_until;
        let held = actors[i].f.held(tick);
        if actors[i].f.standing() && !getting_up && !held {
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
                    .filter(|(j, d)| {
                        // A lookout hears the street below without seeing it.
                        *d < spec.aggro && (en.perch || clear_line(solids, me, actors[*j].chest()))
                    })
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
            let id = actors[i].id;
            match foe {
                None => {
                    en.windup = None;
                    // Back to the corner, walking.
                    let to = en.home - here;
                    let flat = DVec2::new(to.x, to.z);
                    if en.perch {
                        // Back to its place if still up on the roof (one
                        // knocked off stays down there).
                        if flat.length() > 1.0 && to.y.abs() < 1.0 {
                            let d = flat.normalize_or_zero() * 0.3;
                            input.move_x = d.x as f32;
                            input.move_z = d.y as f32;
                        }
                    } else if flat.length() > 1.0 {
                        let d = heading(paths, solids, id, tick, here, en.home) * 0.3;
                        input.move_x = d.x as f32;
                        input.move_z = d.y as f32;
                    } else {
                        paths.remove(&id);
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
                                    && (!ranged || seen);
                                let dmg = spec.damage * level_scale(actors[i].f.level);
                                let kind = if ranged { FxKind::Ray } else { FxKind::Punch };
                                let mut e = event(
                                    tick,
                                    kind,
                                    actors[i].id,
                                    actors[t].id,
                                    actors[t].chest(),
                                );
                                if lands {
                                    let (_, defeat, dealt) =
                                        hit(actors, i, t, dmg, false, tick, None);
                                    e.amount = dealt.round() as u16;
                                    e.defeat = defeat;
                                }
                                fx.push(e);
                            }
                        }
                    } else {
                        let (near, far) = if ranged { (7.0, 16.0) } else { (0.0, 1.6) };
                        // The alpha leaps onto a hero who keeps away.
                        if en.kind == EnemyKind::Alpha
                            && tick >= en.next_trick
                            && (LEAP_MIN..LEAP_MAX).contains(&dist)
                            && seen
                            && actors[j].body.grounded
                        {
                            let take_off = tick + ticks(LEAP_WINDUP);
                            let land = take_off + ticks(LEAP_SECONDS);
                            en.leap = Some((take_off, land, here, there));
                            en.next_trick = land + ticks(LEAP_EVERY);
                            en.next_attack = land + ticks(0.8);
                            fx.push(event(
                                tick,
                                FxKind::LeapWindup,
                                actors[i].id,
                                actors[j].id,
                                there,
                            ));
                            actors[i].enemy = Some(en);
                            continue;
                        }
                        let mut go = DVec2::ZERO;
                        if en.perch {
                            // Holds its place up there; out of sight, it
                            // steps up to the edge to look down, but never
                            // past it.
                            if !seen {
                                let ahead = DVec2::new(here.x, here.z) + dir * 0.8;
                                if solids.floor_under(ahead, here.y + 0.5) > here.y - 0.3 {
                                    go = dir * spec.speed;
                                }
                            }
                        } else if dist > far || !seen {
                            go = heading(paths, solids, id, tick, here, there) * spec.speed;
                        } else if dist < near {
                            go = -dir * spec.speed * 0.8;
                        }
                        let can_attack = dist <= spec.reach && seen && tick >= en.next_attack;
                        if can_attack {
                            let at = tick + ticks(spec.windup);
                            en.windup = Some((at, actors[j].id));
                            en.next_attack = at + ticks(spec.attack_every);
                            let kind = if ranged {
                                FxKind::RayWindup
                            } else {
                                FxKind::PunchWindup
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

/// The alpha's leap: it lands on the hero who kept away (between these
/// distances, m), after a crouch everyone can see.
pub const LEAP_MIN: f64 = 7.0;
pub const LEAP_MAX: f64 = 24.0;
pub const LEAP_WINDUP: f64 = 0.9;
pub const LEAP_SECONDS: f64 = 1.1;
pub const LEAP_EVERY: f64 = 9.0;
/// The landing hurts everyone this near (m), by this share of a punch.
pub const LEAP_AREA: f64 = 4.5;
const LEAP_DAMAGE: f32 = 1.5;

/// One tick of an alpha's leap: a crouch, an arc, a landing that hurts.
fn leap(actors: &mut [Actor], i: usize, en: &mut Enemy, tick: u64, fx: &mut Vec<Fx>) {
    let Some((take_off, land, from, to)) = en.leap else {
        return;
    };
    if !actors[i].f.standing() || actors[i].f.held(tick) {
        en.leap = None;
        return;
    }
    if tick < take_off {
        actors[i].body.vel = DVec3::ZERO;
        return;
    }
    let span = (land - take_off).max(1) as f64;
    let t = ((tick - take_off) as f64 / span).min(1.0);
    let arc = |t: f64| from.lerp(to, t) + DVec3::Y * (4.0 * 6.0 * t * (1.0 - t));
    let next = arc(((tick + 1 - take_off) as f64 / span).min(1.0));
    let now = arc(t);
    actors[i].body.pos = now;
    actors[i].body.vel = (next - now) / DT;
    actors[i].body.grounded = false;
    if tick < land {
        return;
    }
    en.leap = None;
    actors[i].body.pos = to;
    actors[i].body.vel = DVec3::ZERO;
    actors[i].body.grounded = true;
    let me = actors[i].id;
    let dmg = enemy_spec(EnemyKind::Alpha).damage * LEAP_DAMAGE * level_scale(actors[i].f.level);
    for k in 0..actors.len() {
        let a = &actors[k];
        if a.hero.is_none() || !a.f.standing() || a.body.pos.distance(to) > LEAP_AREA {
            continue;
        }
        let (_, defeat, dealt) = hit(actors, i, k, dmg, false, tick, None);
        let mut e = event(tick, FxKind::Leap, me, actors[k].id, actors[k].chest());
        e.amount = dealt.round() as u16;
        e.defeat = defeat;
        fx.push(e);
    }
    fx.push(event(tick, FxKind::Leap, me, me, to));
}

fn statuses(
    actors: &mut [Actor],
    drones: &[(Entity, NetId, Body, Drone)],
    tick: u64,
    out: &mut Out,
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
                let carried = actors[i].enemy.is_some_and(|e| e.kind.carried_off());
                if carried && tick >= actors[i].f.until && !fetched(actors[i].id, spawn_drones) {
                    spawn_drones.push((actors[i].id, actors[i].body.pos, false));
                }
            }
            Status::Down => {
                // A teammate next to them holding revive gets them up.
                let me = actors[i].body.pos;
                let helper = actors
                    .iter()
                    .find(|a| {
                        a.hero.is_some_and(|(_, inp)| inp.revive)
                            && a.f.standing()
                            && a.body.pos.distance(me) <= REVIVE_REACH
                    })
                    .map(|a| a.id);
                let fetching = fetched(actors[i].id, spawn_drones);
                let a = &mut actors[i];
                match helper {
                    Some(by) if !fetching => {
                        a.f.revive += DT;
                        if a.f.revive >= REVIVE_SECONDS {
                            a.f.status = Status::Standing;
                            a.f.health = a.f.max_health * REVIVE_HEALTH;
                            a.f.revive = 0.0;
                            a.f.last_combat = tick;
                            out.fx
                                .push(event(tick, FxKind::Revived, a.id, a.id, a.chest()));
                            out.rewards.push(Reward {
                                hero: by,
                                karma: 0,
                                deed: Some(Affinity::Guardian),
                                lean: 25,
                            });
                        }
                    }
                    _ => {
                        a.f.revive = (a.f.revive - DT).max(0.0);
                        let asked = a.hero.is_some_and(|(_, inp)| inp.hospital);
                        if (asked || tick >= a.f.until) && !fetching {
                            spawn_drones.push((a.id, a.body.pos, true));
                        }
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
        let machine = a
            .enemy
            .is_some_and(|e| matches!(e.kind, EnemyKind::Colossus | EnemyKind::WeakPoint));
        let (h, e) = if a.enemy.is_some() {
            // Villains heal only once nobody fights them (machines never).
            (
                if calm && a.f.target.is_none() && !machine {
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
        let r = roster(Seed(3), 5, 1);
        assert_eq!(r, roster(Seed(3), 5, 1));
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
        assert_eq!(f.view(0).health, 1);
        f.health = HERO_HEALTH;
        assert_eq!(f.view(0).health, 255);
    }
}
