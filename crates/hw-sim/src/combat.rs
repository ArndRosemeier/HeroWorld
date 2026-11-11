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
use crate::factions;
use crate::powers::{self, def_of, Delivery, Element, Fields, Lands, PowerDef};
use crate::robots::{self, Colossus, Mount};
use crate::{
    nav, step_hero, Control, Hero, Ids, LastChange, Motion, Net, Orientation, Progress, Prop,
    Solids, DT, GROUND_Y, TICK_HZ,
};
use bevy_ecs::prelude::*;
use glam::{DVec2, DVec3};
use hw_core::{NetId, Seed};
use hw_net::{
    fizzle, CombatState, EntityKind, Fx, FxKind, InputState, PlayerId, Power, PowerId, Status,
    EFFECT_ARMOURED, EFFECT_BURNING, EFFECT_CAN_FLY, EFFECT_CARRYING, EFFECT_CHARMED,
    EFFECT_CHILLED, EFFECT_CONFUSED, EFFECT_DISARMED, EFFECT_EMPOWERED, EFFECT_FEARED,
    EFFECT_FROZEN, EFFECT_GLIDE, EFFECT_HELD, EFFECT_HIDDEN, EFFECT_RALLIED, EFFECT_SHIELD,
    EFFECT_SPRINT, EFFECT_SUPER_JUMP, EFFECT_SUPER_SPEED, EFFECT_TOUGH, EFFECT_WALL_RUN,
    EFFECT_WEAKENED, POWER_IDS,
};
use std::collections::BTreeMap;

/// A hero's health and energy at level 1.
pub const HERO_HEALTH: f32 = 100.0;
pub const HERO_ENERGY: f32 = 100.0;
/// No second power while the last one plays out (seconds).
pub const ACTION_LOCK: f64 = 0.5;
/// A power that cannot fire (out of range, recharging) waits this long.
pub const QUEUE_SECONDS: f64 = 1.0;
/// A power asked for while it recharges waits for it, if it is ready
/// within this many seconds (so pressing again just after a shot fires it
/// as soon as it can, not never).
pub const RECHARGE_WAIT: f64 = 3.0;
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
/// The medical drone across town: its speed, how fast it comes down onto
/// the hospital's bay, and how far it climbs again before it is gone.
pub const DRONE_CRUISE: f64 = 40.0;
pub const DRONE_DESCEND: f64 = 7.0;
pub const DRONE_AWAY: f64 = 25.0;
/// Seconds a hero floats in a recovery pod before stepping out healed.
pub const POD_SECONDS: f64 = 5.0;
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
/// A lifted thing not thrown in this long slips from the hero's grip and
/// falls just ahead (s): nothing is held for ever.
pub const CARRY_SECONDS: f64 = 8.0;
/// Rubble torn up to throw is gone this long after it lands.
pub const RUBBLE_SECONDS: f64 = 20.0;
/// Gravity in Engine's physics (for the arc of a throw).
const PHYSICS_GRAVITY: f64 = 9.81;

/// A minion frozen by a freeze ray stays in its ice this long (s); one hit
/// while frozen shatters it for this much more damage.
pub const FROZEN_SECONDS: f64 = 3.5;
pub const SHATTER: f32 = 2.0;
/// Tougher villains (and the rest of a cone) are chilled instead: slower
/// on their feet and slower to swing, for this long (s).
pub const CHILL_SECONDS: f64 = 6.0;
/// The slow a freeze ray's chill is ([`CHILL_WINDUP`] is for this much).
pub const CHILL_SLOW: f64 = 0.5;
pub const CHILL_WINDUP: f64 = 1.6;
/// How far a teleport reaches (m).
pub const TELEPORT_RANGE: f64 = 40.0;
/// How far a rescue bubble reaches (m), and how long it floats (s).
pub const BUBBLE_RANGE: f64 = 30.0;
pub const BUBBLE_SECONDS: f64 = 3.0;
/// A hypnotised minion fights for the hero this long (s); a lieutenant for
/// this share of it; a boss shrugs it off.
pub const CHARM_SECONDS: f64 = 8.0;
pub const CHARM_LIEUTENANT: f64 = 0.4;
/// A hypnotised villain looks for foes this far round it (m).
pub const CHARM_REACH: f64 = 25.0;

/// Seconds between calling a meteor down and its landing.
pub const METEOR_DELAY: f64 = 1.2;
/// Rally reaches friends this near (m), who then hit this much harder, get
/// energy back this much faster and their powers recharge sooner.
pub const RALLY_RADIUS: f64 = 20.0;
pub const RALLY_DAMAGE: f32 = 1.35;
pub const RALLY_ENERGY: f32 = 1.5;
/// Powers recharge this much faster in a Rally aura.
pub const RALLY_HASTE: f64 = 1.4;
/// Rank resistance to holds, slows, confusion, fear and charm: the share
/// of the seconds that sticks (minion, lieutenant, boss).
pub const RANK_RESIST: [f64; 3] = [1.0, 0.5, 0.15];
/// A Guardian Angel catches a fatal blow once in this long (s).
pub const ANGEL_EVERY: f64 = 120.0;
/// Seconds between two second winds ([`powers::Fields::second_wind`]),
/// and the share of health that sets one off.
pub const SECOND_WIND_EVERY: f64 = 90.0;
pub const SECOND_WIND_BELOW: f32 = 0.25;
/// Seconds between two times a hero gets up on their own
/// ([`powers::Fields::get_up`]).
pub const GET_UP_EVERY: f64 = 120.0;
/// A hero moving slower than this (m/s) stands still (a brace holds).
pub const STILL_SPEED: f64 = 0.5;
/// How far a bounce, a chain or a spread leaps (m).
pub const SPREAD_REACH: f64 = 8.0;
/// A burn deals its damage in steps this long (s).
pub const BURN_STEP: f64 = 0.5;

pub use crate::powers::Kit;

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
    /// A Chrome Jacket: a street-gang greaser, a melee rusher.
    Greaser,
    /// A Velvet Syndicate gunsel: ranged, with a ray pistol.
    Gunsel,
    /// A Molehaven delver: a stocky tunneller with a pick, melee.
    Delver,
    /// A sky pirate buccaneer: a cutlass, quick on its feet.
    Buccaneer,
    /// A sky pirate gunner: ranged, with a ray pistol.
    Gunner,
    /// A sky pirate captain: a lieutenant with a heavy cutlass.
    Captain,
    /// An Iron-Masked legionary: armoured, with a ray rifle.
    Legionary,
    /// An Iron-Masked praetorian: a lieutenant with a shock lance.
    Praetorian,
    /// The Doctor himself: an archvillain with a Tesla staff, who throws
    /// lightning and now and then discharges an Aether storm round himself
    /// (see [`DOCTOR_STORM_EVERY`]). Only the last chapter of his story
    /// brings him out (see [`crate::story`]).
    Doctor,
    /// The mad scientist's shield bearer: blows from afar glance off its
    /// shield; up close it is a brute like the rest.
    Bulwark,
    /// The mad scientist's grenadier: lobs a bomb at where a hero stands,
    /// which bursts a moment later (step out of the red ring).
    Grenadier,
    /// The mad scientist's lab orderly: no fighter, it patches up the most
    /// hurt of its group from the back row. Beat it first.
    Orderly,
    /// A Zorrak zapper: an invader from the moon with a ray gun.
    Zapper,
    /// A Zorrak grabber: an invader with a grabber claw, up close.
    Grabber,
    /// A Zorrak overlord: a lieutenant who fires a brain ray from the back
    /// row.
    Overlord,
}

/// The Doctor's Aether storm: how long he charges it (the telegraph: get
/// out of the ring), how often, and how hard it hits (times his blow). Its
/// ring is the colossus core's, so it is drawn alike.
pub const DOCTOR_STORM_WINDUP: f64 = 1.8;
pub const DOCTOR_STORM_EVERY: f64 = 9.0;
pub const DOCTOR_STORM_AREA: f64 = robots::PULSE_AREA;
pub const DOCTOR_STORM_HIT: f32 = 1.6;

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
            damage: 26.0,
            attack_every: 6.0,
            windup: 1.6,
            reach: 170.0,
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
        EnemyKind::Greaser => EnemySpec {
            name: "Chrome Jacket",
            rank: "Minion",
            health: 44.0,
            damage: 6.0,
            attack_every: 1.8,
            windup: 0.45,
            reach: 2.2,
            speed: 0.9,
            aggro: 15.0,
            karma: 6.0,
        },
        EnemyKind::Gunsel => EnemySpec {
            name: "Syndicate Gunsel",
            rank: "Minion",
            health: 36.0,
            damage: 6.5,
            attack_every: 2.4,
            windup: 0.6,
            reach: 24.0,
            speed: 0.65,
            aggro: 18.0,
            karma: 6.0,
        },
        EnemyKind::Delver => EnemySpec {
            name: "Molehaven Delver",
            rank: "Minion",
            health: 50.0,
            damage: 6.5,
            attack_every: 2.0,
            windup: 0.55,
            reach: 2.4,
            speed: 0.75,
            aggro: 14.0,
            karma: 6.0,
        },
        EnemyKind::Buccaneer => EnemySpec {
            name: "Sky Pirate Buccaneer",
            rank: "Minion",
            health: 42.0,
            damage: 6.5,
            attack_every: 1.7,
            windup: 0.4,
            reach: 2.5,
            speed: 0.95,
            aggro: 16.0,
            karma: 6.0,
        },
        EnemyKind::Gunner => EnemySpec {
            name: "Sky Pirate Gunner",
            rank: "Minion",
            health: 34.0,
            damage: 6.5,
            attack_every: 2.4,
            windup: 0.6,
            reach: 26.0,
            speed: 0.65,
            aggro: 20.0,
            karma: 6.0,
        },
        EnemyKind::Captain => EnemySpec {
            name: "Sky Pirate Captain",
            rank: "Lieutenant",
            health: 160.0,
            damage: 11.0,
            attack_every: 1.9,
            windup: 0.5,
            reach: 2.7,
            speed: 0.9,
            aggro: 22.0,
            karma: 22.0,
        },
        EnemyKind::Legionary => EnemySpec {
            name: "Iron Legionary",
            rank: "Minion",
            health: 48.0,
            damage: 7.5,
            attack_every: 2.6,
            windup: 0.7,
            reach: 28.0,
            speed: 0.6,
            aggro: 26.0,
            karma: 8.0,
        },
        EnemyKind::Doctor => EnemySpec {
            name: "The Doctor",
            rank: "Archvillain",
            health: 1400.0,
            damage: 15.0,
            attack_every: 2.0,
            windup: 0.6,
            reach: 30.0,
            speed: 0.6,
            aggro: 40.0,
            karma: 220.0,
        },
        EnemyKind::Praetorian => EnemySpec {
            name: "Iron Praetorian",
            rank: "Lieutenant",
            health: 190.0,
            damage: 12.0,
            attack_every: 2.2,
            windup: 0.6,
            reach: 3.0,
            speed: 0.75,
            aggro: 26.0,
            karma: 24.0,
        },
        EnemyKind::Bulwark => EnemySpec {
            name: "Mutant Bulwark",
            rank: "Minion",
            health: 52.0,
            damage: 7.0,
            attack_every: 2.2,
            windup: 0.55,
            reach: 2.4,
            speed: 0.7,
            aggro: 15.0,
            karma: 8.0,
        },
        EnemyKind::Grenadier => EnemySpec {
            name: "Lab Grenadier",
            rank: "Minion",
            health: 32.0,
            // The burst, on everyone in it.
            damage: 11.0,
            attack_every: 4.2,
            windup: 0.7,
            reach: 18.0,
            speed: 0.6,
            aggro: 20.0,
            karma: 7.0,
        },
        EnemyKind::Orderly => EnemySpec {
            name: "Lab Orderly",
            rank: "Minion",
            health: 30.0,
            // A weak ray between mendings ([`ORDERLY_MENDS`]).
            damage: 5.0,
            attack_every: 2.6,
            windup: 0.7,
            reach: 20.0,
            speed: 0.65,
            aggro: 18.0,
            karma: 7.0,
        },
        EnemyKind::Zapper => EnemySpec {
            name: "Zorrak Zapper",
            rank: "Minion",
            health: 42.0,
            damage: 7.0,
            attack_every: 2.4,
            windup: 0.7,
            reach: 26.0,
            speed: 0.7,
            aggro: 24.0,
            karma: 7.0,
        },
        EnemyKind::Grabber => EnemySpec {
            name: "Zorrak Grabber",
            rank: "Minion",
            health: 50.0,
            damage: 7.0,
            attack_every: 2.0,
            windup: 0.55,
            // The claw's pole reaches a little further than a fist.
            reach: 2.9,
            speed: 0.85,
            aggro: 22.0,
            karma: 7.5,
        },
        EnemyKind::Overlord => EnemySpec {
            name: "Zorrak Overlord",
            rank: "Lieutenant",
            health: 180.0,
            damage: 11.0,
            attack_every: 2.4,
            windup: 0.8,
            reach: 24.0,
            speed: 0.55,
            aggro: 26.0,
            karma: 24.0,
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
            EnemyKind::Greaser => EntityKind::Greaser,
            EnemyKind::Gunsel => EntityKind::Gunsel,
            EnemyKind::Delver => EntityKind::Delver,
            EnemyKind::Buccaneer => EntityKind::Buccaneer,
            EnemyKind::Gunner => EntityKind::Gunner,
            EnemyKind::Captain => EntityKind::Captain,
            EnemyKind::Legionary => EntityKind::Legionary,
            EnemyKind::Praetorian => EntityKind::Praetorian,
            EnemyKind::Doctor => EntityKind::Doctor,
            EnemyKind::Bulwark => EntityKind::Bulwark,
            EnemyKind::Grenadier => EntityKind::Grenadier,
            EnemyKind::Orderly => EntityKind::Orderly,
            EnemyKind::Zapper => EntityKind::Zapper,
            EnemyKind::Grabber => EntityKind::Grabber,
            EnemyKind::Overlord => EntityKind::Overlord,
        }
    }

    /// Machines and bosses: no knockback, no police drone for the machines.
    pub fn heavy(self) -> bool {
        !matches!(
            self,
            EnemyKind::Brute
                | EnemyKind::Technician
                | EnemyKind::Greaser
                | EnemyKind::Gunsel
                | EnemyKind::Delver
                | EnemyKind::Buccaneer
                | EnemyKind::Gunner
                | EnemyKind::Legionary
                | EnemyKind::Bulwark
                | EnemyKind::Grenadier
                | EnemyKind::Orderly
                | EnemyKind::Zapper
                | EnemyKind::Grabber
        )
    }

    /// Shoots from a distance.
    pub fn ranged(self) -> bool {
        matches!(
            self,
            EnemyKind::Technician
                | EnemyKind::Sentry
                | EnemyKind::Gunsel
                | EnemyKind::Gunner
                | EnemyKind::Legionary
                | EnemyKind::Doctor
                | EnemyKind::Orderly
                | EnemyKind::Zapper
                | EnemyKind::Overlord
        )
    }

    /// Keeps to the back row and its distance, like a shooter: the
    /// shooters, the grenadier and the orderly.
    pub fn back_row(self) -> bool {
        self.ranged() || matches!(self, EnemyKind::Grenadier | EnemyKind::Orderly)
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
            EntityKind::Greaser => Some(EnemyKind::Greaser),
            EntityKind::Gunsel => Some(EnemyKind::Gunsel),
            EntityKind::Delver => Some(EnemyKind::Delver),
            EntityKind::Buccaneer => Some(EnemyKind::Buccaneer),
            EntityKind::Gunner => Some(EnemyKind::Gunner),
            EntityKind::Captain => Some(EnemyKind::Captain),
            EntityKind::Legionary => Some(EnemyKind::Legionary),
            EntityKind::Praetorian => Some(EnemyKind::Praetorian),
            EntityKind::Doctor => Some(EnemyKind::Doctor),
            EntityKind::Bulwark => Some(EnemyKind::Bulwark),
            EntityKind::Grenadier => Some(EnemyKind::Grenadier),
            EntityKind::Orderly => Some(EnemyKind::Orderly),
            EntityKind::Zapper => Some(EnemyKind::Zapper),
            EntityKind::Grabber => Some(EnemyKind::Grabber),
            EntityKind::Overlord => Some(EnemyKind::Overlord),
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
    /// Tick each power is ready again (by [`PowerId`]).
    pub ready: [u64; POWER_IDS],
    /// A power waiting to fire, and the last tick it may still fire on.
    pub queued: Option<(PowerId, u64)>,
    /// Why the queued power has not fired yet (a [`hw_net::fizzle`] reason;
    /// 0 for none): told to the hero if it lapses.
    pub waiting: u16,
    /// Tick the hero was last told why a power did not fire (once a second
    /// at most, with a key held down).
    pub told: u64,
    /// A power key or button was down last tick (the log notes presses,
    /// not every tick a key is held).
    pub pressing: bool,
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
    /// Behind a force shield until this tick, which stops this share of
    /// every blow and sends this share back.
    pub shield_until: u64,
    pub shield_block: f32,
    pub shield_reflect: f32,
    /// What it holds over its head to throw, and the power that lifted it.
    pub carrying: Option<NetId>,
    pub lifted_with: PowerId,
    /// Tick the held thing slips free if not thrown first.
    pub carry_until: u64,
    /// Effects a robot's state adds ([`EFFECT_ARMOURED`] and the like).
    pub marks: u32,
    /// Inside a friend's aura this tick (a hero's own counts): how much
    /// harder it hits, and how much less it takes.
    pub rallied: bool,
    pub rally: f32,
    pub warded: f32,
    /// Frozen solid in ice until this tick (a freeze ray).
    pub frozen_until: u64,
    /// Slowed until this tick, by this share (cold, a stomp).
    pub chilled_until: u64,
    pub slow: f32,
    /// Fighting for the heroes until this tick (a hypno ray), and for whom.
    pub charmed_until: u64,
    pub charmed_by: Option<NetId>,
    /// Burning until this tick: damage a second, and whose.
    pub burn_until: u64,
    pub burn: f32,
    pub burn_by: Option<NetId>,
    /// Wandering confused until this tick (attacks nobody).
    pub confused_until: u64,
    /// The next tick a psychic blow may confuse it again.
    pub psychic_ready: u64,
    /// Running away until this tick, from there.
    pub feared_until: u64,
    pub fear_from: DVec3,
    /// Cannot attack until this tick.
    pub disarmed_until: u64,
    /// Takes this share more until this tick (a hex).
    pub weak_until: u64,
    pub weak: f32,
    /// A hero's own power on themselves until this tick (a titan, a
    /// dodge, a verdict): less taken, harder hits, never knocked, minions
    /// at one blow.
    pub buff_until: u64,
    pub buff_resist: f32,
    pub buff_might: f32,
    pub buff_steadfast: bool,
    pub buff_execute: bool,
    /// Villains lose sight of this hero until this tick (or a blow).
    pub hidden_until: u64,
    /// The next tick a Guardian Angel may catch a fatal blow.
    pub angel_ready: u64,
    /// The next tick a second wind may come.
    pub wind_ready: u64,
    /// The next tick the hero may get up on their own, and the tick they
    /// last went down.
    pub rise_ready: u64,
    pub down_at: u64,
    /// Movement this tick for the fight to answer: a super jump took off,
    /// and how fast (m/s down) a super jumper hit the ground hard (0: no
    /// hard landing). Set by the hero's movement, cleared by [`step`].
    pub leapt: bool,
    pub landed: f32,
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
            ready: [0; POWER_IDS],
            queued: None,
            waiting: 0,
            told: 0,
            pressing: false,
            target: None,
            revive: 0.0,
            engaged: false,
            kit: Kit::default(),
            held_until: 0,
            shield_until: 0,
            shield_block: 0.0,
            shield_reflect: 0.0,
            carrying: None,
            lifted_with: PowerId::STRIKE,
            carry_until: 0,
            marks: 0,
            rallied: false,
            rally: 0.0,
            warded: 0.0,
            frozen_until: 0,
            chilled_until: 0,
            slow: 0.0,
            charmed_until: 0,
            charmed_by: None,
            burn_until: 0,
            burn: 0.0,
            burn_by: None,
            confused_until: 0,
            psychic_ready: 0,
            feared_until: 0,
            fear_from: DVec3::ZERO,
            disarmed_until: 0,
            weak_until: 0,
            weak: 0.0,
            buff_until: 0,
            buff_resist: 0.0,
            buff_might: 0.0,
            buff_steadfast: false,
            buff_execute: false,
            hidden_until: 0,
            angel_ready: 0,
            wind_ready: 0,
            rise_ready: 0,
            down_at: 0,
            leapt: false,
            landed: 0.0,
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

    /// Frozen in ice at `tick`.
    pub fn frozen(&self, tick: u64) -> bool {
        tick < self.frozen_until
    }

    /// Slowed by cold at `tick` (frozen counts).
    pub fn chilled(&self, tick: u64) -> bool {
        tick < self.chilled_until || self.frozen(tick)
    }

    /// Hypnotised: fighting for the heroes at `tick`.
    pub fn charmed(&self, tick: u64) -> bool {
        tick < self.charmed_until
    }

    /// How much slower on its feet at `tick` (0 to 1; frozen: 1).
    pub fn slowed(&self, tick: u64) -> f32 {
        if self.frozen(tick) {
            1.0
        } else if tick < self.chilled_until {
            self.slow
        } else {
            0.0
        }
    }

    pub fn confused(&self, tick: u64) -> bool {
        tick < self.confused_until
    }

    pub fn feared(&self, tick: u64) -> bool {
        tick < self.feared_until
    }

    pub fn disarmed(&self, tick: u64) -> bool {
        tick < self.disarmed_until
    }

    pub fn burning(&self, tick: u64) -> bool {
        tick < self.burn_until
    }

    pub fn hidden(&self, tick: u64) -> bool {
        tick < self.hidden_until
    }

    /// Its own power on itself at `tick`.
    pub fn buffed(&self, tick: u64) -> bool {
        tick < self.buff_until
    }

    /// Out of the fight for now: bound, frozen, confused or afraid (a
    /// takedown hits such a one harder).
    pub fn helpless(&self, tick: u64) -> bool {
        self.held(tick) || self.frozen(tick) || self.confused(tick) || self.feared(tick)
    }

    /// How much harder a hero hits at `tick`: their might, a buff, a
    /// friend's aura.
    pub fn outgoing(&self, tick: u64) -> f32 {
        let buff = if self.buffed(tick) {
            self.buff_might
        } else {
            0.0
        };
        (1.0 + self.kit.passive.might + buff) * (1.0 + self.rally)
    }

    /// The share of a blow that gets through at `tick` (a hero's own
    /// toughness, a buff, a friend's ward).
    pub fn takes(&self, tick: u64) -> f32 {
        let buff = if self.buffed(tick) {
            self.buff_resist
        } else {
            0.0
        };
        (1.0 - self.kit.passive.resist) * (1.0 - buff) * (1.0 - self.warded)
    }

    /// How much faster powers recharge (a friend's rally, a gadget belt).
    pub fn haste(&self) -> f64 {
        (1.0 + self.kit.passive.haste as f64) * if self.rallied { RALLY_HASTE } else { 1.0 }
    }

    /// Never knocked down at `tick`.
    pub fn steadfast(&self, tick: u64) -> bool {
        self.kit.passive.steadfast || (self.buffed(tick) && self.buff_steadfast)
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
            effects: (self.held(tick) as u32 * EFFECT_HELD)
                | (self.shielded(tick) as u32 * EFFECT_SHIELD)
                | (self.carrying.is_some() as u32 * EFFECT_CARRYING)
                | (self.kit.travels(powers::travel::FLY) as u32 * EFFECT_CAN_FLY)
                | (self.kit.travels(powers::travel::JUMP) as u32 * EFFECT_SUPER_JUMP)
                | (self.kit.travels(powers::travel::SPEED) as u32 * EFFECT_SUPER_SPEED)
                | (self.kit.travels(powers::travel::SPRINT) as u32 * EFFECT_SPRINT)
                | (self.kit.travels(powers::travel::GLIDE) as u32 * EFFECT_GLIDE)
                | (self.kit.travels(powers::travel::WALL) as u32 * EFFECT_WALL_RUN)
                | ((self.kit.passive.resist >= 0.3) as u32 * EFFECT_TOUGH)
                | (self.rallied as u32 * EFFECT_RALLIED)
                | (self.frozen(tick) as u32 * EFFECT_FROZEN)
                | ((self.chilled(tick) && !self.frozen(tick)) as u32 * EFFECT_CHILLED)
                | (self.charmed(tick) as u32 * EFFECT_CHARMED)
                | (self.burning(tick) as u32 * EFFECT_BURNING)
                | (self.confused(tick) as u32 * EFFECT_CONFUSED)
                | (self.feared(tick) as u32 * EFFECT_FEARED)
                | (self.disarmed(tick) as u32 * EFFECT_DISARMED)
                | ((tick < self.weak_until) as u32 * EFFECT_WEAKENED)
                | (self.hidden(tick) as u32 * EFFECT_HIDDEN)
                | (self.buffed(tick) as u32 * EFFECT_EMPOWERED)
                | self.marks,
            toggles: self.kit.toggles_on(),
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
    /// The Doctor's Aether storm goes off at this tick.
    pub storm: Option<u64>,
    /// Its group's turf: it warns heroes off (see [`crate::factions`]).
    pub turf: bool,
    /// A hero this near provokes it (stepping into its crime, or a fight
    /// that is on).
    pub provoke: Option<f64>,
    /// The hero its group warned off, and when.
    pub warned: Option<(NetId, u64)>,
    /// Which of its kind's looks it wears, when it must be a particular
    /// one (a street crook stepped in on, who was drawn before: see
    /// [`crate::crimes`]); else its id picks.
    pub look: Option<u8>,
    /// Making off from a crime nobody stopped: it hurries to its `home`,
    /// far down the street, and is gone there (see [`crate::incidents`]).
    pub leaving: bool,
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
    /// Where it is on its way once it has them.
    pub leg: Leg,
}

/// A drone's way once it has picked someone up.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Leg {
    /// Straight up (and gone, for the police and where there is no
    /// hospital).
    #[default]
    Up,
    /// Up to `high` and across town at that height to over the bay of
    /// hospital `ward`, then down onto it.
    ToHospital {
        ward: usize,
        high: f64,
    },
    Landing {
        ward: usize,
    },
    /// The hero handed over: up and gone, from this height.
    Away {
        from: f64,
    },
}

/// A hero healing in a hospital's recovery pod.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Patient {
    pub id: NetId,
    pub ward: usize,
    pub pod: usize,
    pub since: u64,
    pub until: u64,
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
    /// A faction's own crew in place of brutes and technicians: (who
    /// fights up close, who shoots). `only` wins over it.
    pub crew: Option<(EnemyKind, EnemyKind)>,
    pub once: bool,
    /// On a roof (`center` on top of it): its group stands up there.
    pub perch: bool,
    /// A lieutenant or boss in front of the group.
    pub lead: Option<EnemyKind>,
    /// A hero this near provokes the group: a crime's reach (stepping in),
    /// or unbounded for a fight that is on. Without it, the group acts on
    /// its faction's regard for the hero (see [`crate::factions`]).
    pub provoke: Option<f64>,
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
            crew: None,
            once: false,
            perch: false,
            lead: None,
            provoke: None,
        }
    }

    /// A group that fights any hero it notices (tests, the balance runs).
    pub fn fight(center: DVec3, facing: DVec2) -> Self {
        Self {
            provoke: Some(f64::INFINITY),
            ..Self::corner(center, facing)
        }
    }
}

/// Karma a hero earned this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reward {
    pub hero: NetId,
    pub karma: u32,
}

/// A hero was there when one of a faction's people was beaten: the
/// faction's regard for them moves (see [`crate::factions::beat`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Deed {
    pub hero: NetId,
    pub beaten: hw_net::Faction,
}

/// Something thrown, on its way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Throw {
    /// The tick it lands.
    pub land: u64,
    pub from: NetId,
    pub to: NetId,
    pub damage: f32,
    /// The power that threw it.
    pub power: PowerId,
}

/// A strike called down from the sky (Meteor), on its way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyStrike {
    /// The tick it lands.
    pub land: u64,
    pub from: NetId,
    /// Where it lands (on the ground).
    pub at: DVec3,
    pub damage: f32,
    /// The power that called it.
    pub power: PowerId,
    /// The hero's level (how hard it hits the city).
    pub level: u8,
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
    /// What the server did with heroes' power keys in the last tick, in
    /// words (the debug menu's log; cleared each tick).
    pub power_log: Vec<String>,
    pub seed: u64,
    /// The level heroes fight at (the host's hero's): new groups come at it.
    pub team_level: u8,
    /// Karma earned in the last tick (the simulation pays it out).
    pub rewards: Vec<Reward>,
    /// Villains beaten in the last tick, by hero (moves factions' regard).
    pub deeds: Vec<Deed>,
    /// Factions a hero helped in the last tick, and by how much (talking
    /// frightened mole folk down).
    pub friends: Vec<(NetId, hw_net::Faction, i16)>,
    /// Throws in the air.
    pub throws: Vec<Throw>,
    /// Meteors on their way down.
    pub sky: Vec<SkyStrike>,
    /// Villains' paths round walls.
    pub paths: BTreeMap<NetId, Path>,
    /// Blows at the city this tick (the colossus's feet and shells):
    /// (where, radius, power); the simulation breaks buildings with them.
    pub city: Vec<(DVec3, f32, f32)>,
    /// The streets the colossus walks (none on a bare test ground).
    pub streets: Option<std::sync::Arc<crate::streets::Streets>>,
    /// The route maps of the buildings with game-scale interiors:
    /// villains find their way through rooms and up stairs by them.
    pub floors: Option<std::sync::Arc<crate::floornav::Floors>>,
    /// Each colossus's route along them: (planned at tick, points left).
    pub routes: BTreeMap<NetId, (u64, Vec<DVec2>)>,
    /// Where people wait to be rescued (trapped, or lying hurt), set by the
    /// incidents each tick: a rescue bubble needs one in reach.
    pub rescue_points: Vec<DVec3>,
    /// Rescue bubbles sent this tick (the incidents carry them out).
    pub rescues: Vec<Rescue>,
    /// Freeze-ray cones this tick: (from, direction, length); a fire in
    /// one goes out.
    pub cold: Vec<(DVec3, DVec3, f64)>,
    /// Grenadiers' bombs in the air.
    pub bombs: Vec<Bomb>,
    /// Where Hero's Return takes a hero (Hero Hall's plaza, else the
    /// town's centre).
    pub home: Option<DVec3>,
    pub home_is_hall: bool,
    /// The town's hospitals: where the medical drone sets heroes down and
    /// heals them (none on a bare test ground: heroes come back to the
    /// spawn street there).
    pub wards: Vec<hw_gen::urban::hospital::Ward>,
    /// Heroes healing in the pods.
    pub patients: Vec<Patient>,
    /// The square round the heroes, for flying over its roofs.
    pub area: Option<Skyline>,
    /// Screenshots: the first hero goes straight into a recovery pod and
    /// floats there for ten minutes.
    pub pod_start: bool,
    /// Every hero's portal home and open pair (see [`crate::gates`]).
    pub gates: crate::gates::Gates,
}

/// The heroes' square of the town, for flying over its roofs.
#[derive(Clone)]
pub struct Skyline(pub std::sync::Arc<hw_gen::urban::area::Area>);

impl std::fmt::Debug for Skyline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Skyline")
    }
}

/// A grenadier's bomb on its way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bomb {
    /// The tick it bursts.
    pub land: u64,
    pub from: NetId,
    /// Where it bursts (on the ground where its target stood).
    pub at: DVec3,
    pub damage: f32,
}

/// A bomb bursts this long after the grenadier starts its throw (s): the
/// windup plus the flight. Long enough to step out of the ring.
pub const BOMB_SECONDS: f64 = 1.9;
/// Everyone this near the burst is caught (m).
pub const BOMB_AREA: f64 = 2.6;
/// A shield bearer's shield stops blows from farther than this (m) that
/// come from in front (within [`SHIELD_ARC`] degrees of its foe's way).
pub const SHIELD_REACH: f64 = 3.5;
pub const SHIELD_ARC: f64 = 70.0;
/// What a blow on the shield still does.
pub const SHIELD_LETS_THROUGH: f32 = 0.25;
/// A lab orderly mends this often (s), a villain of its group within this
/// reach (m) below this share of its health, by this much (at its level).
pub const ORDERLY_EVERY: f64 = 4.0;
pub const ORDERLY_REACH: f64 = 14.0;
pub const ORDERLY_BELOW: f32 = 0.8;
pub const ORDERLY_MENDS: f32 = 14.0;

impl Default for Fights {
    fn default() -> Self {
        Self {
            spots: Vec::new(),
            events: Vec::new(),
            power_log: Vec::new(),
            seed: 0,
            team_level: 1,
            rewards: Vec::new(),
            deeds: Vec::new(),
            friends: Vec::new(),
            throws: Vec::new(),
            sky: Vec::new(),
            paths: BTreeMap::new(),
            city: Vec::new(),
            streets: None,
            floors: None,
            routes: BTreeMap::new(),
            rescue_points: Vec::new(),
            rescues: Vec::new(),
            cold: Vec::new(),
            bombs: Vec::new(),
            home: None,
            home_is_hall: false,
            wards: Vec::new(),
            patients: Vec::new(),
            area: None,
            pod_start: false,
            gates: Default::default(),
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
    /// Who beat it this tick.
    beaten_by: Option<NetId>,
    /// A weak point's robot.
    pub(crate) mount: Option<Mount>,
    /// The walking tank's own state.
    pub(crate) colossus: Option<Colossus>,
    /// A hero's deeds as factions see them (the sheet's offsets).
    pub(crate) regard: [i8; hw_net::FACTIONS],
    /// One of the town's own heroes ([`crate::npcheroes`]).
    pub(crate) npc: bool,
    /// A hero's robot sidekick ([`crate::sidekick`]).
    pub(crate) sidekick: Option<crate::sidekick::Sidekick>,
    /// Its ice shattered under a blow this tick.
    pub(crate) shattered: bool,
    /// Blows its shield stopped this tick: (from, what got through).
    pub(crate) blocked: Vec<(NetId, f32)>,
    /// A guardian angel caught a fatal blow on it this tick.
    pub(crate) saved: bool,
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

/// The mad scientist's groups get roles (by `seed`): a shield bearer in
/// place of a brute from four up; from five, a grenadier in place of a
/// technician or an orderly in place of a brute, both from seven. Other
/// factions' crews keep their own two kinds.
pub fn with_roles(seed: Seed, group: &mut [(EnemyKind, u8)]) {
    let mut rng = seed.child("roles").rng();
    let n = group.len();
    let mut swap = |k: usize, from: EnemyKind, to: EnemyKind| {
        if let Some(g) = group.get_mut(k).filter(|g| g.0 == from) {
            g.0 = to;
        }
    };
    if n >= 4 {
        swap(2, EnemyKind::Brute, EnemyKind::Bulwark);
    }
    let (grenadier, orderly) = match n {
        0..=4 => (false, false),
        5 | 6 => {
            let g = rng.chance(0.5);
            (g, !g)
        }
        _ => (true, true),
    };
    if grenadier {
        swap(3, EnemyKind::Technician, EnemyKind::Grenadier);
    }
    if orderly {
        swap(4, EnemyKind::Brute, EnemyKind::Orderly);
    }
}

/// Group size for `heroes` players: bigger groups, not tougher minions.
pub fn group_size(heroes: usize) -> u32 {
    (5 + heroes.saturating_sub(1) as u32).min(8)
}

/// A [`fizzle`] reason in words, for the debug log.
fn why_words(why: u16) -> &'static str {
    match why {
        fizzle::NO_TARGET => "no target",
        fizzle::OUT_OF_RANGE => "out of range",
        fizzle::NOT_SEEN => "target not in sight",
        fizzle::RECHARGING => "recharging",
        fizzle::ENERGY => "not enough energy",
        fizzle::NO_SPOT => "no spot",
        fizzle::BUSY => "the last power still plays out",
        fizzle::NO_ROOM => "no room for a portal",
        fizzle::NO_HOME => "no portal home marked",
        _ => "no reason noted",
    }
}

/// A queued power cannot fire yet for `why`: noted, and logged when the
/// reason changes.
fn note_wait(f: &mut Fighter, why: u16, power: &str, tick: u64, log: &mut Vec<String>) {
    if f.waiting != why {
        log.push(format!("server {tick}: {power} waits: {}", why_words(why)));
    }
    f.waiting = why;
}

/// Can a fighter whose chest is at `a` see one whose chest is at `b`? Yes
/// if the line from chest to chest is clear, or the line from head to head
/// ([`HEAD_ABOVE_CHEST`] higher): a parked car, a hedge, a cafe table or a
/// news stand hides the chest but not the head and shoulders, and a hero
/// who can see a crook over a car can shoot it (and be shot).
pub fn in_sight(solids: &Obstacles, a: DVec3, b: DVec3) -> bool {
    let up = DVec3::Y * HEAD_ABOVE_CHEST;
    clear_line(solids, a, b) || clear_line(solids, a + up, b + up)
}

/// How far above the chest point (60% of the body height) the head is (m).
pub const HEAD_ABOVE_CHEST: f64 = BODY_HEIGHT * 0.3;

/// Is the straight line from `a` to `b` free of walls?
/// Thin walls count (see [`Obstacles::line_blocked`]); the last few
/// centimetres at each end do not, so a fighter brushing a wall still sees.
pub fn clear_line(solids: &Obstacles, a: DVec3, b: DVec3) -> bool {
    const END: f64 = 0.1;
    let d = b - a;
    let len = d.length();
    if len <= 2.0 * END {
        return true;
    }
    let step = d * (END / len);
    !solids.line_blocked(a + step, b - step)
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
        .map(|(kind, level)| {
            let kind = match (spot.only, spot.crew) {
                (Some(only), _) => only,
                (None, Some((close, far))) => {
                    if kind.ranged() {
                        far
                    } else {
                        close
                    }
                }
                (None, None) => kind,
            };
            (kind, level)
        })
        .collect();
    if spot.only.is_none() && spot.crew.is_none() {
        with_roles(gseed, &mut group);
    }
    // A lieutenant or boss takes the place of the first in line.
    if let (Some(lead), Some(first)) = (spot.lead, group.first_mut()) {
        *first = (
            lead,
            level.saturating_add(1).min(crate::progress::MAX_LEVEL),
        );
    }
    let count = group.len();
    let mut taken: Vec<DVec2> = Vec::new();
    for (k, (kind, level)) in group.into_iter().enumerate() {
        // Brutes in a loose front row, shooters a few metres behind.
        let row = match kind {
            k if k.back_row() => -3.5,
            EnemyKind::Colossus => 0.0,
            _ => 1.5,
        };
        let across = if kind == EnemyKind::Colossus {
            0.0
        } else {
            (k as f64 - (count as f64 - 1.0) * 0.5) * 1.6
        };
        let mut p = DVec2::new(spot.center.x, spot.center.z) + spot.facing * row + side * across;
        // On open ground in sight of the spot, never inside a house or a
        // wall (a shop's robbers stand in front of it, not in its rooms).
        if !spot.perch && kind != EnemyKind::Colossus {
            p = open_stand(&solids, spot.center, p, &taken);
            taken.push(p);
        }
        let y = if spot.perch {
            solids.floor_under(p, spot.center.y + 1.0)
        } else {
            // The street, or the floor of the tunnel or cavern it is in.
            let street = solids.ground_at(p).max(GROUND_Y);
            let floor = solids.ground_for(p, spot.center.y + 1.0);
            if floor < street - 1.0 {
                floor
            } else {
                street
            }
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

/// Is there room for a body standing on the ground at `p` (no wall, house,
/// closed door or rock round its feet, middle or head)?
pub fn room_to_stand(solids: &Obstacles, p: DVec2, y: f64) -> bool {
    let r = crate::collision::BODY_RADIUS;
    [0.3, 1.0, 1.6].iter().all(|&h| {
        [(0.0, 0.0), (r, 0.0), (-r, 0.0), (0.0, r), (0.0, -r)]
            .iter()
            .all(|&(x, z)| !solids.solid_at(DVec3::new(p.x + x, y + h, p.y + z)))
    })
}

/// Where a villain of a group at `center` stands, near `want`: `want`
/// itself if there is room there, on the spot's own ground, in sight of the
/// spot and clear of the others (`taken`); else the nearest such place
/// within 12 m; else `want`. Every crook of a call stands where a hero can
/// see and reach them.
pub fn open_stand(solids: &Obstacles, center: DVec3, want: DVec2, taken: &[DVec2]) -> DVec2 {
    let ground = |p: DVec2| {
        let street = solids.ground_at(p).max(GROUND_Y);
        let floor = solids.ground_for(p, center.y + 1.0);
        if floor < street - 1.0 {
            floor
        } else {
            street
        }
    };
    let chest = DVec3::Y * (BODY_HEIGHT * 0.6);
    let anchor = center + chest;
    // A spot that is itself in a wall has no sight to keep: just get out.
    let seen = room_to_stand(solids, DVec2::new(center.x, center.z), center.y);
    let ok = |p: DVec2| {
        let y = ground(p);
        (y - center.y).abs() < 1.5
            && room_to_stand(solids, p, y)
            && taken.iter().all(|t| t.distance(p) >= 1.0)
            && (!seen || in_sight(solids, anchor, DVec3::new(p.x, y, p.y) + chest))
    };
    if ok(want) {
        return want;
    }
    const RINGS: u32 = 24;
    const ANGLES: u32 = 24;
    for ring in 1..=RINGS {
        let r = ring as f64 * 0.5;
        // Nearest first round each ring, starting on the side towards the
        // spot (out of the house, back to the street).
        let to = (DVec2::new(center.x, center.z) - want).normalize_or(DVec2::X);
        let a0 = hw_core::det::atan2(to.y, to.x);
        for k in 0..ANGLES {
            let step = k.div_ceil(2);
            let sign = if k % 2 == 0 { 1.0 } else { -1.0 };
            let a = a0 + sign * step as f64 * std::f64::consts::TAU / ANGLES as f64;
            let p = want + DVec2::new(hw_core::det::cos(a), hw_core::det::sin(a)) * r;
            if ok(p) {
                return p;
            }
        }
    }
    want
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
    let (spot_turf, spot_provoke) = world
        .resource::<Fights>()
        .spots
        .get(spot)
        .map_or((false, None), |s| (s.wake.is_some(), s.provoke));
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
            storm: None,
            turf: spot_turf,
            provoke: spot_provoke,
            warned: None,
            look: None,
            leaving: false,
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
    /// What happened to each hero's power keys, in words (the debug log).
    log: Vec<String>,
    /// Heroes (by actor index) reaching for something to lift.
    lifts: Vec<usize>,
    /// Lifted things let go: (what, velocity).
    tosses: Vec<(NetId, DVec3)>,
    /// New throws in the air.
    throws: Vec<Throw>,
    /// New meteors called down.
    sky: Vec<SkyStrike>,
    /// Blows at the city (where, radius, power).
    city: Vec<(DVec3, f32, f32)>,
    rewards: Vec<Reward>,
    deeds: Vec<Deed>,
    /// Sidekicks called: (owner, player, power, which of how many, where,
    /// level).
    summons: Vec<Summon>,
    /// Rescue bubbles sent (see [`Fights::rescues`]).
    rescues: Vec<Rescue>,
    /// Freeze-ray cones (see [`Fights::cold`]).
    cold: Vec<(DVec3, DVec3, f64)>,
}

/// A sidekick called.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Summon {
    pub owner: NetId,
    pub player: PlayerId,
    pub power: PowerId,
    /// Which of the power's sidekicks this is (the first sends the old
    /// ones home).
    pub index: u8,
    pub at: DVec3,
    pub level: u8,
}

/// A rescue bubble sent: the hero, and the trapped or hurt person's place
/// it was sent to (the rescue there takes them out).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rescue {
    pub hero: NetId,
    pub level: u8,
    pub at: DVec3,
}

/// One tick of every fight in `world` (after heroes moved).
pub(crate) fn step(world: &mut World, tick: u64) {
    world.resource_mut::<Fights>().events.clear();
    world.resource_mut::<Fights>().rewards.clear();
    world.resource_mut::<Fights>().deeds.clear();
    world.resource_mut::<Fights>().friends.clear();
    world.resource_mut::<Fights>().rescues.clear();
    world.resource_mut::<Fights>().cold.clear();
    let solids = world.resource::<Solids>().0.clone();
    let mut actors: Vec<Actor> = world
        .query::<(
            Entity,
            &Net,
            &Motion,
            &Fighter,
            Option<&Enemy>,
            Option<(&Hero, &Control)>,
            (
                Option<&Mount>,
                Option<&Colossus>,
                Option<&Progress>,
                Has<crate::npcheroes::NpcHero>,
                Option<&crate::sidekick::Sidekick>,
            ),
        )>()
        .iter(world)
        .map(
            |(e, n, m, f, en, h, (mount, colossus, progress, npc, sidekick))| Actor {
                e,
                id: n.0,
                body: m.0,
                f: *f,
                enemy: en.copied(),
                hero: h.map(|(h, c)| (h.owner, c.0)),
                beaten_by: None,
                mount: mount.copied(),
                colossus: colossus.copied(),
                regard: progress.map_or([0; hw_net::FACTIONS], |p| p.sheet.regard),
                npc,
                sidekick: sidekick.copied(),
                shattered: false,
                blocked: Vec::new(),
                saved: false,
            },
        )
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
    let mut sky = std::mem::take(&mut world.resource_mut::<Fights>().sky);
    let mut bombs = std::mem::take(&mut world.resource_mut::<Fights>().bombs);

    rally(&mut actors);
    let rescue_points = std::mem::take(&mut world.resource_mut::<Fights>().rescue_points);
    let home = world.resource::<Fights>().home;
    let mut gates = std::mem::take(&mut world.resource_mut::<Fights>().gates);
    // Who stepped through a portal this tick (heroes move before combat).
    for c in gates.take_crossed() {
        let owner = actors
            .iter()
            .find(|a| a.hero.is_some_and(|h| h.0 == c.owner) && !a.npc)
            .map_or(c.hero, |a| a.id);
        let mut e = event(tick, FxKind::GateCross, owner, c.hero, c.at);
        e.theme = PowerId::GATE_OPEN.0;
        e.amount = crate::gates::yaw_turns(c.turn);
        out.fx.push(e);
    }
    heroes_act(
        &mut actors,
        &solids,
        &rescue_points,
        home,
        &mut gates,
        tick,
        &mut out,
    );
    world.resource_mut::<Fights>().gates = gates;
    leaps(&mut actors, &solids, tick, &mut out);
    world.resource_mut::<Fights>().rescue_points = rescue_points;
    npc_heroes_act(&mut actors, &solids, tick, &mut out);
    sidekicks_act(&mut actors, &solids, tick, &mut out);
    land_throws(&mut actors, &mut throws, tick, &mut out);
    throws.append(&mut out.throws);
    land_sky(&mut actors, &mut sky, &solids, tick, &mut out);
    sky.append(&mut out.sky);
    let floors = world.resource::<Fights>().floors.clone();
    enemies_act(
        &mut actors,
        &solids,
        floors.as_deref(),
        tick,
        &mut out.fx,
        &mut paths,
        &mut bombs,
    );
    land_bombs(&mut actors, &mut bombs, &solids, tick, &mut out);
    let mut city = std::mem::take(&mut out.city);
    let streets = world.resource::<Fights>().streets.clone();
    let mut routes = std::mem::take(&mut world.resource_mut::<Fights>().routes);
    robots::act(
        &mut actors,
        &solids,
        streets.as_deref(),
        &mut routes,
        tick,
        &mut out.fx,
        &mut city,
    );
    routes.retain(|id, _| actors.iter().any(|a| a.id == *id && a.colossus.is_some()));
    world.resource_mut::<Fights>().routes = routes;
    burns(&mut actors, tick, &mut out);
    statuses(&mut actors, &drones, tick, &mut out, &mut spawn_drones);
    // Ice broken by a blow this tick.
    for a in actors.iter_mut().filter(|a| a.shattered) {
        a.shattered = false;
        out.fx
            .push(event(tick, FxKind::Shatter, a.id, a.id, a.chest()));
    }
    // Blows that hit a shield.
    for a in actors.iter_mut() {
        for (from, through) in std::mem::take(&mut a.blocked) {
            let mut e = event(tick, FxKind::Block, from, a.id, a.chest());
            e.amount = through.round() as u16;
            out.fx.push(e);
        }
    }
    regenerate(&mut actors, tick);
    pay_out(&actors, &mut out);
    // Villains gone (carried off) need no path.
    paths.retain(|id, _| actors.iter().any(|a| a.id == *id && a.enemy.is_some()));
    {
        let mut f = world.resource_mut::<Fights>();
        f.paths = paths;
        f.throws = throws;
        f.sky = sky;
        f.bombs = bombs;
    }
    let mut fx = std::mem::take(&mut out.fx);

    // Drones fly: down to whoever they fetch, then up and away; the medical
    // one across town to the nearest hospital's bay.
    let mut gone: Vec<(Entity, NetId)> = Vec::new();
    let mut back_to_hospital: Vec<NetId> = Vec::new();
    let (wards, area) = {
        let f = world.resource::<Fights>();
        (f.wards.clone(), f.area.clone())
    };
    let mut patients = std::mem::take(&mut world.resource_mut::<Fights>().patients);
    for (de, did, body, d) in drones.iter_mut() {
        let Some(i) = actors.iter().position(|a| a.id == d.carry) else {
            gone.push((*de, *did));
            continue;
        };
        if d.lifting.is_none() {
            let want = actors[i].body.pos + DVec3::Y * DRONE_HANG;
            let to = want - body.pos;
            let step = DRONE_SPEED * DT;
            if to.length() <= step {
                body.pos = want;
                body.vel = DVec3::ZERO;
                d.lifting = Some(want.y);
                d.leg = match nearest_ward(&wards, want) {
                    Some(ward) if d.medical => Leg::ToHospital {
                        ward,
                        high: cruise_height(area.as_ref().map(|a| &*a.0), want, wards[ward].bay),
                    },
                    _ => Leg::Up,
                };
                let a = &mut actors[i];
                a.f.status = Status::Carried;
                a.body.vel = DVec3::ZERO;
                fx.push(event(tick, FxKind::Lifted, *did, a.id, a.chest()));
            } else {
                body.vel = to.normalize() * DRONE_SPEED;
                body.pos += body.vel * DT;
            }
            continue;
        }
        let from = d.lifting.unwrap_or(body.pos.y);
        match d.leg {
            Leg::Up => {
                body.vel = DVec3::Y * DRONE_LIFT_SPEED;
                body.pos += body.vel * DT;
                if body.pos.y > from + DRONE_LIFT_HEIGHT {
                    gone.push((*de, *did));
                    if d.medical {
                        back_to_hospital.push(actors[i].id);
                    } else {
                        gone.push((actors[i].e, actors[i].id));
                    }
                }
            }
            Leg::ToHospital { ward, high } => {
                let bay = wards[ward].bay + DVec3::Y * DRONE_HANG;
                if body.pos.y < high - 0.01 {
                    // Up first, drifting off towards the hospital.
                    body.vel = DVec3::Y * DRONE_LIFT_SPEED * 2.0;
                    body.pos.y = (body.pos.y + body.vel.y * DT).min(high);
                } else {
                    let flat = DVec3::new(bay.x - body.pos.x, 0.0, bay.z - body.pos.z);
                    let left = flat.length();
                    // Easing in over the last stretch.
                    let speed = DRONE_CRUISE.min(left * 0.8 + 2.0);
                    if left <= speed * DT {
                        body.pos.x = bay.x;
                        body.pos.z = bay.z;
                        body.vel = DVec3::ZERO;
                        d.leg = Leg::Landing { ward };
                    } else {
                        body.vel = flat / left * speed;
                        body.pos += body.vel * DT;
                    }
                }
            }
            Leg::Landing { ward } => {
                let bay = wards[ward].bay + DVec3::Y * DRONE_HANG;
                let down = (body.pos.y - bay.y).max(0.0);
                let speed = DRONE_DESCEND.min(down * 0.9 + 0.6);
                if down <= speed * DT {
                    // Set down at the bay: the hero goes into a pod.
                    body.pos = bay;
                    body.vel = DVec3::ZERO;
                    d.leg = Leg::Away { from: bay.y };
                    let pod = free_pod(&patients, ward, wards[ward].pods.len());
                    let a = &mut actors[i];
                    a.body.pos = wards[ward].pods[pod].0;
                    a.body.vel = DVec3::ZERO;
                    patients.retain(|p| p.id != a.id);
                    patients.push(Patient {
                        id: a.id,
                        ward,
                        pod,
                        since: tick,
                        until: tick + ticks(POD_SECONDS),
                    });
                    fx.push(event(tick, FxKind::Admitted, a.id, a.id, a.chest()));
                    continue;
                }
                body.vel = -DVec3::Y * speed;
                body.pos += body.vel * DT;
            }
            Leg::Away { from } => {
                body.vel = DVec3::Y * DRONE_LIFT_SPEED * 1.5;
                body.pos += body.vel * DT;
                if body.pos.y > from + DRONE_AWAY {
                    gone.push((*de, *did));
                }
                // The hero is in the pod now, not under the drone.
                continue;
            }
        }
        let a = &mut actors[i];
        a.body.pos = body.pos - DVec3::Y * DRONE_HANG;
        a.body.vel = body.vel;
        a.body.grounded = false;
    }
    if world.resource::<Fights>().pod_start {
        let hero = actors.iter().position(|a| a.hero.is_some());
        if let Some((a, ward)) =
            hero.and_then(|i| Some((i, nearest_ward(&wards, actors[i].body.pos)?)))
        {
            let a = &mut actors[a];
            world.resource_mut::<Fights>().pod_start = false;
            a.f.status = Status::Carried;
            a.f.health = a.f.max_health * 0.3;
            let pod = free_pod(&patients, ward, wards[ward].pods.len());
            patients.push(Patient {
                id: a.id,
                ward,
                pod,
                since: tick,
                until: tick + ticks(600.0),
            });
        }
    }
    // A hero left hanging with no drone and no pod (a drone lost when
    // the host changed): straight into a pod, or home where there is no
    // hospital.
    for a in actors.iter_mut() {
        let held = drones.iter().any(|d| {
            d.3.carry == a.id
                && !matches!(d.3.leg, Leg::Away { .. })
                && !gone.iter().any(|g| g.0 == d.0)
        });
        if a.hero.is_none()
            || a.f.status != Status::Carried
            || held
            || patients.iter().any(|p| p.id == a.id)
            || back_to_hospital.contains(&a.id)
        {
            continue;
        }
        match nearest_ward(&wards, a.body.pos) {
            Some(ward) => {
                let pod = free_pod(&patients, ward, wards[ward].pods.len());
                patients.push(Patient {
                    id: a.id,
                    ward,
                    pod,
                    since: tick,
                    until: tick + ticks(POD_SECONDS),
                });
                fx.push(event(tick, FxKind::Admitted, a.id, a.id, a.chest()));
            }
            None => back_to_hospital.push(a.id),
        }
    }
    // Heroes in the pods float there, getting better; out they step when
    // their time is up.
    let mut healed: Vec<(NetId, DVec3)> = Vec::new();
    patients.retain(|p| {
        let Some(a) = actors.iter_mut().find(|a| a.id == p.id) else {
            return false;
        };
        let Some(&(at, out, _)) = wards.get(p.ward).and_then(|w| w.pods.get(p.pod)) else {
            return false;
        };
        if a.f.status != Status::Carried {
            return false;
        }
        a.body.pos = at;
        a.body.vel = DVec3::ZERO;
        a.body.grounded = true;
        let k = (tick.saturating_sub(p.since)) as f32 / (p.until - p.since).max(1) as f32;
        a.f.health = a.f.health.max(a.f.max_health * k.min(1.0));
        if tick >= p.until {
            healed.push((p.id, out));
            return false;
        }
        true
    });
    world.resource_mut::<Fights>().patients = patients;

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
        let mut e = event(
            tick,
            FxKind::Lift,
            hero.id,
            id,
            at + DVec3::Y * CARRY_HEIGHT,
        );
        e.theme = hero.f.lifted_with.0;
        e.element = hero.f.kit.element.code();
        fx.push(e);
        actors[i].f.carrying = Some(id);
        actors[i].f.carry_until = tick + ticks(CARRY_SECONDS);
    }
    // Lifted things ride over their heroes' heads, and fall if they fall.
    // Held too long, they slip free and drop just ahead.
    let mut riding: BTreeMap<NetId, Body> = BTreeMap::new();
    let mut dropped: Vec<NetId> = Vec::new();
    for a in actors.iter_mut() {
        let Some(c) = a.f.carrying else {
            continue;
        };
        if a.f.standing() && tick >= a.f.carry_until {
            a.f.carrying = None;
            let flat = DVec3::new(a.body.vel.x, 0.0, a.body.vel.z).normalize_or(DVec3::X);
            out.tosses
                .push((c, a.body.vel + flat * 3.0 + DVec3::Y * 1.5));
            // The power recharges as after a throw.
            let p = a.f.lifted_with;
            a.f.ready[p.0 as usize] = tick + ticks(powers::recharge(def_of(p)) / a.f.haste());
            let mut e = event(
                tick,
                FxKind::Throw,
                a.id,
                a.id,
                a.body.pos + DVec3::Y * CARRY_HEIGHT,
            );
            e.theme = p.0;
            e.element = a.f.kit.element.code();
            fx.push(e);
        } else if a.f.standing() {
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
                leg: Leg::Up,
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
    for (id, out) in healed {
        let found = world
            .query::<(Entity, &Net, &Fighter)>()
            .iter(world)
            .find(|(_, n, _)| n.0 == id)
            .map(|(e, _, _)| e);
        let Some(e) = found else {
            continue;
        };
        let mut em = world.entity_mut(e);
        if let Some(mut m) = em.get_mut::<Motion>() {
            m.0 = Body {
                pos: out,
                vel: DVec3::ZERO,
                grounded: true,
            };
        }
        if let Some(mut f) = em.get_mut::<Fighter>() {
            let mut fresh = Fighter::hero_at(f.level, f.kit);
            fresh.last_combat = tick;
            *f = fresh;
            fx.push(event(tick, FxKind::Healed, id, id, out.as_vec3()));
        }
    }
    for id in back_to_hospital {
        let found = world
            .query::<(Entity, &Net, &Fighter)>()
            .iter(world)
            .find(|(_, n, _)| n.0 == id)
            .map(|(e, _, f)| (e, f.level));
        let Some((e, level)) = found else {
            continue;
        };
        // New heroes come back on their own street.
        let hospital = world
            .resource::<crate::SpawnLine>()
            .hospital(&solids, level);
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
    // Sidekicks called to their heroes' side.
    for sm in std::mem::take(&mut out.summons) {
        crate::sidekick::spawn(world, sm, tick);
    }
    robots::tidy(world, tick);
    let mut f = world.resource_mut::<Fights>();
    f.events = fx;
    f.power_log = out.log;
    f.rewards = out.rewards;
    f.deeds = out.deeds;
    f.city = city;
    f.rescues = out.rescues;
    f.cold = out.cold;
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
        element: 0,
    }
}

/// Deals `damage` from actor `from` to actor `to` (a shield takes its
/// share), knocking it back if `knock` (or if the blow defeats it).
/// Returns (knocked, defeated, damage dealt).
pub(crate) fn hit(
    actors: &mut [Actor],
    from: usize,
    to: usize,
    damage: f32,
    knock: bool,
    tick: u64,
) -> (bool, bool, f32) {
    strike(actors, from, to, damage, knock, tick, false)
}

/// [`hit`] for a single aimed blow (a bolt, a punch, a beam from Hero
/// Hall's heroes): a shield
/// bearer's shield stops it when it comes from afar and from in front.
/// Bursts, beams, throws and meteors go round or through a shield.
pub(crate) fn hit_aimed(
    actors: &mut [Actor],
    from: usize,
    to: usize,
    damage: f32,
    knock: bool,
    tick: u64,
) -> (bool, bool, f32) {
    strike(actors, from, to, damage, knock, tick, true)
}

fn strike(
    actors: &mut [Actor],
    from: usize,
    to: usize,
    damage: f32,
    knock: bool,
    tick: u64,
    blockable: bool,
) -> (bool, bool, f32) {
    let from_id = actors[from].id;
    let from_pos = actors[from].body.pos;
    // A hypnotised villain's or a sidekick's win is their hero's.
    let credit = actors[from]
        .f
        .charmed_by
        .filter(|_| actors[from].f.charmed(tick))
        .or(actors[from].sidekick.map(|s| s.owner))
        .unwrap_or(from_id);
    actors[from].f.last_combat = tick;
    // A hero who strikes is seen again.
    if actors[from].hero.is_some() {
        actors[from].f.hidden_until = 0;
    }
    // A hero's verdict: minions fall at a blow.
    let executes =
        actors[from].hero.is_some() && actors[from].f.buffed(tick) && actors[from].f.buff_execute;
    // A shield bearer holds its shield toward the one it fights.
    let shield_way = actors[to]
        .f
        .target
        .and_then(|f| actors.iter().find(|a| a.id == f))
        .map(|f| f.body.pos);
    let t = &mut actors[to];
    let mut dealt = damage;
    // A hex: everyone's blows land harder.
    if tick < t.f.weak_until {
        dealt *= 1.0 + t.f.weak;
    }
    let mut reflected = 0.0;
    if t.f.shielded(tick) {
        reflected += dealt * t.f.shield_reflect;
        dealt *= 1.0 - t.f.shield_block;
    }
    // Frozen: the ice shatters, and the blow lands twice as hard.
    if t.f.frozen(tick) {
        dealt *= SHATTER;
        t.f.frozen_until = 0;
        t.shattered = true;
    }
    // Shut armour: blows glance off.
    if t.f.marks & EFFECT_ARMOURED != 0 {
        dealt *= robots::ARMOUR_LETS_THROUGH;
    }
    // A hero's own toughness, buffs and friends' wards (a sidekick has
    // only the wards).
    if t.hero.is_some() {
        let p = t.f.kit.passive;
        // A dodge: the blow misses (the same every run).
        if p.dodge > 0.0 && roll(tick.wrapping_add((from_id.0 as u64) << 32), t.id) < p.dodge as f64
        {
            dealt = 0.0;
        }
        // Braced: standing still, the hero takes less.
        let speed = DVec2::new(t.body.vel.x, t.body.vel.z).length();
        if p.brace > 0.0 && speed < STILL_SPEED && t.body.grounded {
            dealt *= 1.0 - p.brace;
        }
        reflected += dealt * p.reflect;
        dealt *= t.f.takes(tick);
    } else if t.sidekick.is_some() {
        dealt *= t.f.takes(tick);
    }
    let minion = t
        .enemy
        .is_some_and(|e| rank_of(e.kind) == 0 && !e.kind.heavy());
    if executes && minion {
        dealt = dealt.max(t.f.health);
    }
    let heavy = t.enemy.is_some_and(|e| e.kind.heavy());
    let mut knock = knock && !heavy;
    if blockable && on_shield(t, shield_way, from_pos, tick) {
        dealt *= SHIELD_LETS_THROUGH;
        knock = false;
        t.blocked.push((from_id, dealt));
    }
    // The town's own heroes are never beaten (they go home first).
    let floor = if t.npc { 1.0 } else { 0.0 };
    // A guardian angel: once in a while a fatal blow leaves a hero up.
    let angel = t.hero.is_some()
        && t.f.kit.passive.angel
        && tick >= t.f.angel_ready
        && t.f.health - dealt <= 0.0
        && t.f.standing();
    let floor = if angel {
        t.f.angel_ready = tick + ticks(ANGEL_EVERY);
        t.saved = true;
        1.0
    } else {
        floor
    };
    t.f.health = (t.f.health - dealt).max(floor);
    t.f.last_combat = tick;
    // A second wind: near defeat, a share of health comes back.
    let wind = t.f.kit.passive.second_wind;
    if t.hero.is_some()
        && wind > 0.0
        && t.f.health > 0.0
        && t.f.health < t.f.max_health * SECOND_WIND_BELOW
        && tick >= t.f.wind_ready
    {
        t.f.wind_ready = tick + ticks(SECOND_WIND_EVERY);
        t.f.health = (t.f.health + t.f.max_health * wind).min(t.f.max_health);
        t.saved = true;
    }
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
        t.f.frozen_until = 0;
        t.f.charmed_until = 0;
        t.f.burn_until = 0;
        t.f.confused_until = 0;
        t.f.feared_until = 0;
        t.beaten_by = Some(credit);
    } else if defeated {
        t.f.status = Status::Down;
        t.f.until = tick + ticks(MEDIC_DELAY);
        t.f.queued = None;
        t.f.revive = 0.0;
        t.f.down_at = tick;
        // A downed hero's toggles drop.
        t.f.kit.set_toggles(0);
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
    // A shield or a hero's aegis sends some of the blow back.
    if reflected > 0.0 && from != to && actors[from].f.fighting() {
        let by = actors[to].id;
        let a = &mut actors[from];
        a.f.health = (a.f.health - reflected).max(if a.npc { 1.0 } else { 0.0 });
        if a.f.health <= 0.0 && a.enemy.is_some() {
            a.f.status = Status::Down;
            a.f.until = tick + ticks(POLICE_DELAY);
            a.beaten_by = Some(by);
        }
    }
    // The whole group joins in, except against one of their own turned by
    // the Hypno Ray: only the one it struck turns on it, the rest keep at
    // the hero.
    let turned = actors[from].f.charmed(tick);
    if let Some(spot) = actors[to].enemy.map(|e| e.spot).filter(|_| !turned) {
        for a in actors.iter_mut() {
            if a.enemy.is_some_and(|e| e.spot == spot) && a.f.target.is_none() && a.f.fighting() {
                a.f.target = Some(from_id);
            }
        }
    }
    (knocked, defeated, dealt)
}

/// A villain's rank: 0 minion, 1 lieutenant, 2 boss (machines count as
/// bosses).
pub fn rank_of(k: EnemyKind) -> usize {
    match enemy_spec(k).rank {
        "Minion" if !k.heavy() => 0,
        "Lieutenant" if !k.heavy() => 1,
        _ => 2,
    }
}

/// What a control effect can do to someone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hinder {
    Hold,
    Slow,
    Confuse,
    Fear,
    Charm,
    Disarm,
}

/// The share of a control effect's seconds that sticks on `a`: by rank
/// ([`RANK_RESIST`]); machines shrug off confusion, fear and charm.
pub(crate) fn sticks(a: &Actor, c: Hinder) -> f64 {
    let Some(en) = a.enemy else {
        return 0.0;
    };
    if en.kind.heavy() && matches!(c, Hinder::Confuse | Hinder::Fear | Hinder::Charm) {
        return 0.0;
    }
    RANK_RESIST[rank_of(en.kind)]
}

/// The town's own heroes bolt their targets from the air.
fn npc_heroes_act(actors: &mut [Actor], solids: &Obstacles, tick: u64, out: &mut Out) {
    use crate::npcheroes::{BOLT_DAMAGE, BOLT_EVERY, BOLT_RANGE};
    for i in 0..actors.len() {
        if !actors[i].npc || !actors[i].f.standing() || tick < actors[i].f.busy_until {
            continue;
        }
        let Some(j) = actors[i]
            .f
            .target
            .and_then(|t| actors.iter().position(|a| a.id == t))
            .filter(|&j| hittable(&actors[j]))
            .filter(|&j| actors[j].body.pos.distance(actors[i].body.pos) <= BOLT_RANGE)
            .filter(|&j| in_sight(solids, actors[i].chest(), actors[j].chest()))
        else {
            continue;
        };
        let damage = BOLT_DAMAGE * level_scale(actors[i].f.level);
        let (knocked, defeated, dealt) = hit_aimed(actors, i, j, damage, false, tick);
        actors[i].f.busy_until = tick + ticks(BOLT_EVERY);
        let mut e = event(
            tick,
            FxKind::Beam,
            actors[i].id,
            actors[j].id,
            actors[j].chest(),
        );
        e.amount = dealt.round() as u16;
        e.knock = knocked;
        e.defeat = defeated;
        out.fx.push(e);
    }
}

/// Sidekicks fight: the villain they were sent at, else their hero's
/// target, else one going for their hero or them. They punch when close
/// (`sidekick` walks them there).
fn sidekicks_act(actors: &mut [Actor], solids: &Obstacles, tick: u64, out: &mut Out) {
    use crate::sidekick::{GUARD_REACH, PUNCH_EVERY, PUNCH_REACH};
    for i in 0..actors.len() {
        let Some(sk) = actors[i].sidekick else {
            continue;
        };
        if !actors[i].f.standing() {
            continue;
        }
        let owner = actors.iter().position(|a| a.id == sk.owner);
        let here = actors[i].body.pos;
        let valid = |a: &Actor| hittable(a) && a.body.pos.distance(here) < 40.0;
        let mut foe = actors[i]
            .f
            .target
            .and_then(|t| actors.iter().position(|a| a.id == t))
            .filter(|&j| valid(&actors[j]));
        if foe.is_none() {
            foe = owner
                .and_then(|o| actors[o].f.target)
                .and_then(|t| actors.iter().position(|a| a.id == t))
                .filter(|&j| valid(&actors[j]) && actors[j].f.target.is_some());
        }
        if foe.is_none() {
            let me = actors[i].id;
            foe = (0..actors.len())
                .filter(|&j| valid(&actors[j]))
                .filter(|&j| actors[j].f.target == Some(sk.owner) || actors[j].f.target == Some(me))
                .map(|j| (j, actors[j].body.pos.distance(here)))
                .filter(|&(_, d)| d < GUARD_REACH)
                .min_by(|x, y| x.1.total_cmp(&y.1).then(x.0.cmp(&y.0)))
                .map(|(j, _)| j);
        }
        actors[i].f.target = foe.map(|j| actors[j].id);
        let Some(j) = foe else {
            continue;
        };
        if tick < actors[i].f.busy_until
            || actors[j].body.pos.distance(here) > PUNCH_REACH
            || !in_sight(solids, actors[i].chest(), actors[j].chest())
        {
            continue;
        }
        let damage = crate::sidekick::PUNCH_DAMAGE * sk.strength * level_scale(actors[i].f.level);
        let (knock, defeat, dealt) = hit(actors, i, j, damage, false, tick);
        actors[i].f.busy_until = tick + ticks(PUNCH_EVERY);
        let mut e = event(
            tick,
            FxKind::Strike,
            actors[i].id,
            actors[j].id,
            actors[j].chest(),
        );
        e.amount = dealt.round() as u16;
        e.knock = knock;
        e.defeat = defeat;
        e.theme = sk.power.0;
        out.fx.push(e);
    }
}

/// Is `a` (by index) a valid target for heroes?
fn hittable(a: &Actor) -> bool {
    a.enemy.is_some() && a.f.fighting() && a.f.charmed_by.is_none()
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

/// The nearest villain within `range` of `chest` (m, measured from the
/// feet) with nothing solid between, if any.
fn nearest_seen_enemy(
    actors: &[Actor],
    solids: &Obstacles,
    chest: DVec3,
    range: f64,
) -> Option<usize> {
    let from = chest - DVec3::Y * (BODY_HEIGHT * 0.6);
    actors
        .iter()
        .enumerate()
        .filter(|(_, a)| hittable(a))
        .map(|(i, a)| (i, a.body.pos.distance(from)))
        .filter(|&(i, d)| d <= range && in_sight(solids, chest, actors[i].chest()))
        .min_by(|x, y| x.1.total_cmp(&y.1).then(x.0.cmp(&y.0)))
        .map(|(i, _)| i)
}

/// The look an event of a hero's power takes, by its mechanic.
fn look(d: &PowerDef) -> FxKind {
    match d.base {
        Power::Strike => FxKind::Strike,
        Power::Bolt => FxKind::Bolt,
        Power::Hold => FxKind::Hold,
        Power::Hypno => FxKind::Hypno,
        Power::Beam => FxKind::Beam,
        Power::Freeze => FxKind::Freeze,
        Power::Nova => FxKind::Nova,
        Power::Meteor => FxKind::Meteor,
        Power::Mend => FxKind::Mend,
        Power::Lift => FxKind::ThrowHit,
        _ => FxKind::Shockwave,
    }
}

/// A number in 0..1 from a tick and an ID, the same every run.
fn roll(tick: u64, id: NetId) -> f64 {
    let mut x = tick
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((id.0 as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9));
    x ^= x >> 31;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 29;
    (x >> 11) as f64 / (1u64 << 53) as f64
}

/// A hero's power `power` landing on villain `t` for `damage` (already
/// scaled by level and might): the blow, what the power does besides
/// (its fields), the hero's element, and the event. `centre` is where an
/// area power went off (pushes and pulls go from there). Returns
/// (knocked, defeated, damage dealt).
#[allow(clippy::too_many_arguments)]
fn blow(
    actors: &mut [Actor],
    i: usize,
    t: usize,
    damage: f32,
    power: PowerId,
    centre: DVec3,
    aimed: bool,
    tick: u64,
    out: &mut Out,
) -> (bool, bool, f32) {
    let d = def_of(power);
    let f = d.fields;
    let mut damage = damage;
    let mine = actors[i].f.kit.passive;
    let exploit = f.exploit + mine.exploit;
    if exploit > 0.0 && actors[t].f.helpless(tick) {
        damage *= 1.0 + exploit;
    }
    // An opening move: a villain not yet fighting this hero takes the
    // first blow harder.
    if mine.ambush > 0.0 && actors[t].f.target != Some(actors[i].id) {
        damage *= 1.0 + mine.ambush;
    }
    let (knock, mut defeat, dealt) = if damage > 0.0 {
        strike(actors, i, t, damage, f.knock, tick, aimed)
    } else {
        (false, false, 0.0)
    };
    // Cuffed: a minion beaten down this far gives up.
    let minion = actors[t]
        .enemy
        .is_some_and(|e| rank_of(e.kind) == 0 && !e.kind.heavy());
    if mine.cuff > 0.0
        && dealt > 0.0
        && !defeat
        && minion
        && actors[t].f.fighting()
        && actors[t].f.health <= actors[t].f.max_health * mine.cuff
    {
        let rest = actors[t].f.health;
        let (_, beaten, _) = strike(actors, i, t, rest, false, tick, false);
        defeat = beaten;
    }
    let charmed = riders(actors, i, t, dealt, defeat, &f, centre, tick);
    if dealt > 0.0 {
        element(actors, i, t, dealt, defeat, tick, out);
    }
    let kind = look(d);
    let mut e = event(tick, kind, actors[i].id, actors[t].id, actors[t].chest());
    e.amount = if kind == FxKind::Hypno {
        charmed.round() as u16
    } else {
        dealt.round() as u16
    };
    e.knock = if kind == FxKind::Freeze {
        actors[t].f.frozen(tick)
    } else {
        knock
    };
    e.defeat = defeat;
    e.theme = power.0;
    e.element = actors[i].f.kit.element.code();
    out.fx.push(e);
    // Spreads and chains leap on to villains near the one hit.
    let leap = if f.spread_to > 0 {
        (f.spread, f.spread_to)
    } else if d.delivery() == Delivery::Melee && actors[i].f.kit.passive.chain > 0.0 {
        (actors[i].f.kit.passive.chain, 1)
    } else {
        (0.0, 0)
    };
    if leap.1 > 0 && dealt > 0.0 {
        let from = actors[t].body.pos;
        let mut near: Vec<(usize, f64)> = (0..actors.len())
            .filter(|&k| k != t && hittable(&actors[k]))
            .map(|k| (k, actors[k].body.pos.distance(from)))
            .filter(|&(_, dist)| dist <= SPREAD_REACH)
            .collect();
        near.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        for (k, _) in near.into_iter().take(leap.1 as usize) {
            let (knock, defeat, got) = strike(actors, i, k, dealt * leap.0, f.knock, tick, false);
            riders(actors, i, k, got, defeat, &f, centre, tick);
            let mut e = event(tick, kind, actors[i].id, actors[k].id, actors[k].chest());
            e.amount = got.round() as u16;
            e.knock = knock;
            e.defeat = defeat;
            e.theme = power.0;
            e.element = actors[i].f.kit.element.code();
            out.fx.push(e);
        }
    }
    (knock, defeat, dealt)
}

/// What a power does to villain `t` besides the damage (a hold, a freeze,
/// a burn, fear, ...), as far as its rank lets it stick, and health back
/// to hero `i` for a drain. Returns the seconds a charm took.
#[allow(clippy::too_many_arguments)]
fn riders(
    actors: &mut [Actor],
    i: usize,
    t: usize,
    dealt: f32,
    defeat: bool,
    f: &Fields,
    centre: DVec3,
    tick: u64,
) -> f64 {
    if f.drain > 0.0 && dealt > 0.0 {
        let h = &mut actors[i].f;
        h.health = (h.health + dealt * f.drain).min(h.max_health);
    }
    if defeat || actors[t].enemy.is_none() {
        return 0.0;
    }
    let me_id = actors[i].id;
    let me_pos = actors[i].body.pos;
    let me_level = actors[i].f.level;
    let share = |c| sticks(&actors[t], c);
    let (hold, slow, confuse, fear, charm, disarm) = (
        share(Hinder::Hold),
        share(Hinder::Slow),
        share(Hinder::Confuse),
        share(Hinder::Fear),
        share(Hinder::Charm),
        share(Hinder::Disarm),
    );
    let heavy = actors[t].enemy.is_some_and(|e| e.kind.heavy());
    let freezes = actors[t].enemy.is_some_and(|e| freezable(e.kind));
    let a = &mut actors[t];
    if f.hold > 0.0 {
        // Tougher villains shake a hold off sooner.
        let con = a.f.level as f64 - me_level as f64;
        let secs = f.hold * (1.0 - 0.15 * con).clamp(0.4, 1.3) * hold;
        a.f.held_until = a.f.held_until.max(tick + ticks(secs));
    }
    if f.freeze > 0.0 && freezes {
        a.f.frozen_until = tick + ticks(f.freeze);
        a.f.held_until = 0;
        if let Some(en) = a.enemy.as_mut() {
            en.windup = None;
        }
    }
    if f.slow > 0.0 && f.slow_secs > 0.0 && slow > 0.0 {
        a.f.slow = if tick < a.f.chilled_until {
            a.f.slow.max(f.slow)
        } else {
            f.slow
        };
        a.f.chilled_until = a.f.chilled_until.max(tick + ticks(f.slow_secs * slow));
    }
    if f.confuse > 0.0 && confuse > 0.0 {
        a.f.confused_until = a.f.confused_until.max(tick + ticks(f.confuse * confuse));
        if let Some(en) = a.enemy.as_mut() {
            en.windup = None;
        }
    }
    if f.fear > 0.0 && fear > 0.0 {
        a.f.feared_until = a.f.feared_until.max(tick + ticks(f.fear * fear));
        a.f.fear_from = me_pos;
        if let Some(en) = a.enemy.as_mut() {
            en.windup = None;
        }
    }
    if f.disarm > 0.0 && disarm > 0.0 {
        a.f.disarmed_until = a.f.disarmed_until.max(tick + ticks(f.disarm * disarm));
        if let Some(en) = a.enemy.as_mut() {
            en.windup = None;
        }
    }
    if f.weaken > 0.0 {
        a.f.weak = if tick < a.f.weak_until {
            a.f.weak.max(f.weaken)
        } else {
            f.weaken
        };
        a.f.weak_until = tick + ticks(f.lasts.max(1.0));
    }
    if f.dot > 0.0 && dealt > 0.0 {
        burn(&mut a.f, dealt * f.dot, me_id, tick);
    }
    if !heavy && (f.launch > 0.0 || f.push != 0.0) {
        let mut away = DVec2::new(a.body.pos.x - centre.x, a.body.pos.z - centre.z);
        let dist = away.length();
        away = away.normalize_or(DVec2::X);
        // A shove of `push` metres (a pull stops at the centre).
        let metres = if f.push < 0.0 {
            -(dist.min(-f.push as f64))
        } else {
            f.push as f64
        };
        let speed = metres * 2.2;
        a.body.vel.x += away.x * speed;
        a.body.vel.z += away.y * speed;
        a.body.vel.y = a
            .body
            .vel
            .y
            .max(f.launch as f64)
            .max(if metres.abs() > 0.0 { 2.0 } else { 0.0 });
        a.body.grounded = false;
    }
    let mut charmed = 0.0;
    if f.charm > 0.0 && charm > 0.0 {
        let secs = f.charm * charm;
        a.f.charmed_until = tick + ticks(secs);
        a.f.charmed_by = Some(me_id);
        a.f.target = None;
        a.f.held_until = 0;
        if let Some(en) = a.enemy.as_mut() {
            en.windup = None;
        }
        let id = a.id;
        // Nobody keeps after the hero's new friend.
        if actors[i].f.target == Some(id) {
            actors[i].f.target = None;
        }
        charmed = secs;
    }
    charmed
}

/// Sets `f` burning for `total` more damage over [`powers::DOT_SECONDS`].
fn burn(f: &mut Fighter, total: f32, by: NetId, tick: u64) {
    let left = if f.burning(tick) {
        f.burn * (f.burn_until - tick) as f32 / TICK_HZ as f32
    } else {
        0.0
    };
    f.burn = (left + total) / powers::DOT_SECONDS as f32;
    f.burn_until = tick + ticks(powers::DOT_SECONDS);
    f.burn_by = Some(by);
}

/// The hero's element riding on a blow of `dealt` from hero `i` on `t`.
fn element(
    actors: &mut [Actor],
    i: usize,
    t: usize,
    dealt: f32,
    defeat: bool,
    tick: u64,
    out: &mut Out,
) {
    if actors[i].hero.is_none() || actors[t].enemy.is_none() {
        return;
    }
    let el = actors[i].f.kit.element;
    let me_id = actors[i].id;
    let mut e = event(
        tick,
        FxKind::Element,
        me_id,
        actors[t].id,
        actors[t].chest(),
    );
    e.element = el.code();
    match el {
        Element::None => return,
        Element::Fire => {
            if defeat {
                return;
            }
            burn(&mut actors[t].f, dealt * powers::FIRE_BURN, me_id, tick);
            e.amount = (dealt * powers::FIRE_BURN).round() as u16;
        }
        Element::Frost => {
            let secs = powers::FROST_SECONDS * sticks(&actors[t], Hinder::Slow);
            if defeat || secs <= 0.0 {
                return;
            }
            let a = &mut actors[t].f;
            a.slow = if tick < a.chilled_until {
                a.slow.max(powers::FROST_SLOW)
            } else {
                powers::FROST_SLOW
            };
            a.chilled_until = a.chilled_until.max(tick + ticks(secs));
        }
        Element::Energy => {
            let heavy = actors[t].enemy.is_some_and(|e| e.kind.heavy());
            if heavy || actors[t].f.status == Status::Down && !defeat {
                return;
            }
            let from = actors[i].body.pos;
            let a = &mut actors[t];
            let away =
                DVec2::new(a.body.pos.x - from.x, a.body.pos.z - from.z).normalize_or(DVec2::X);
            let speed = powers::ENERGY_PUSH as f64 * 4.0;
            a.body.vel.x += away.x * speed;
            a.body.vel.z += away.y * speed;
        }
        Element::Necrotic => {
            let h = &mut actors[i].f;
            let back = dealt * powers::NECROTIC_DRAIN;
            h.health = (h.health + back).min(h.max_health);
            e.amount = back.round().max(1.0) as u16;
        }
        Element::Lightning => {
            let from = actors[t].body.pos;
            let next = (0..actors.len())
                .filter(|&k| k != t && hittable(&actors[k]))
                .map(|k| (k, actors[k].body.pos.distance(from)))
                .filter(|&(_, d)| d <= powers::LIGHTNING_REACH)
                .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
            let Some((k, _)) = next else {
                return;
            };
            let (_, defeat, got) = strike(
                actors,
                i,
                k,
                dealt * powers::LIGHTNING_SHARE,
                false,
                tick,
                false,
            );
            e = event(
                tick,
                FxKind::Element,
                actors[t].id,
                actors[k].id,
                actors[k].chest(),
            );
            e.element = el.code();
            e.amount = got.round() as u16;
            e.defeat = defeat;
        }
        Element::Psychic => {
            let secs = powers::PSYCHIC_SECONDS * sticks(&actors[t], Hinder::Confuse);
            let a = &mut actors[t];
            if defeat
                || secs <= 0.0
                || tick < a.f.psychic_ready
                || roll(tick, a.id) >= powers::PSYCHIC_CHANCE
            {
                return;
            }
            a.f.confused_until = a.f.confused_until.max(tick + ticks(secs));
            a.f.psychic_ready = tick + ticks(powers::PSYCHIC_COOLDOWN);
            if let Some(en) = a.enemy.as_mut() {
                en.windup = None;
            }
            e.knock = true;
        }
    }
    out.fx.push(e);
}

/// Heroes' powers on themselves and friends within `radius` of `at`
/// (0: only the hero): health, a shield, a buff, getting the downed up.
/// Villains there get the power's effects without the damage (a
/// sanctuary slows them).
#[allow(clippy::too_many_arguments)]
fn allies(
    actors: &mut [Actor],
    i: usize,
    power: PowerId,
    at: DVec3,
    level: u8,
    solids: &Obstacles,
    tick: u64,
    out: &mut Out,
) {
    let f = def_of(power).fields;
    let me_id = actors[i].id;
    let element = actors[i].f.kit.element.code();
    let heal = f.heal * level_scale(level);
    let buffs = f.resist > 0.0 || f.might > 0.0 || f.steadfast || f.execute || f.hide;
    for (k, a) in actors.iter_mut().enumerate() {
        let mine = k == i;
        let friend = a.hero.is_some() && a.body.pos.distance(at) <= f.radius;
        if !mine && !friend {
            continue;
        }
        if f.revive && a.f.status == Status::Down && a.hero.is_some() {
            a.f.status = Status::Standing;
            a.f.health = a.f.max_health * REVIVE_HEALTH;
            a.f.revive = 0.0;
            a.f.last_combat = tick;
            out.fx
                .push(event(tick, FxKind::Revived, a.id, a.id, a.chest()));
        }
        if !a.f.fighting() {
            continue;
        }
        let mut kind = None;
        if heal > 0.0 {
            let before = a.f.health;
            a.f.health = (a.f.health + heal).min(a.f.max_health);
            let mut e = event(tick, FxKind::Mend, me_id, a.id, a.chest());
            e.amount = (a.f.health - before).round() as u16;
            kind = Some(e);
        }
        if f.shield > 0.0 && f.lasts > 0.0 {
            a.f.shield_until = tick + ticks(f.lasts);
            a.f.shield_block = f.shield;
            a.f.shield_reflect = f.reflect;
            kind = Some(event(tick, FxKind::Shield, me_id, a.id, a.chest()));
        }
        // Buffs are the hero's own.
        if buffs && mine && f.lasts > 0.0 {
            a.f.buff_until = tick + ticks(f.lasts);
            a.f.buff_resist = f.resist;
            a.f.buff_might = f.might;
            a.f.buff_steadfast = f.steadfast;
            a.f.buff_execute = f.execute;
            if f.hide {
                a.f.hidden_until = tick + ticks(f.lasts);
            }
            kind = kind.or(Some(event(tick, FxKind::Buff, me_id, a.id, a.chest())));
        }
        if let Some(mut e) = kind {
            e.theme = power.0;
            e.element = element;
            out.fx.push(e);
        }
    }
    // A hidden hero drops out of every villain's sight.
    if f.hide {
        for a in actors.iter_mut() {
            if a.enemy.is_some() && a.f.target == Some(me_id) {
                a.f.target = None;
                if let Some(en) = a.enemy.as_mut() {
                    en.windup = None;
                }
            }
        }
    }
    let controls = f.slow > 0.0 || f.confuse > 0.0 || f.fear > 0.0 || f.hold > 0.0;
    if controls && f.damage <= 0.0 && f.radius > 0.0 {
        for k in 0..actors.len() {
            if hittable(&actors[k])
                && actors[k].body.pos.distance(at) <= f.radius
                && clear_line(solids, at + DVec3::Y, actors[k].chest())
            {
                riders(actors, i, k, 0.0, false, &f, at, tick);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn heroes_act(
    actors: &mut [Actor],
    solids: &Obstacles,
    rescue_points: &[DVec3],
    home: Option<DVec3>,
    gates: &mut crate::gates::Gates,
    tick: u64,
    out: &mut Out,
) {
    for i in 0..actors.len() {
        let Some((_, input)) = actors[i].hero else {
            continue;
        };
        actors[i].f.engaged = false;
        let kit = actors[i].f.kit;
        // Left mouse punches, Q fires the hero's first bolt (held: again
        // and again), the other keys their powers. A power key goes first,
        // and a held button never pushes aside another power waiting to
        // fire (holding the mouse would otherwise swallow every key press).
        let waiting = actors[i].f.queued.map(|q| q.0);
        let quick = |p: PowerId| p == PowerId::STRIKE || Some(p) == kit.blast;
        // A toggle flips once a press: holding its key does not flip it
        // back and forth.
        let fresh = !actors[i].f.pressing;
        let key = Some(PowerId(input.power))
            .filter(|p| p.0 != 0 && kit.has(*p) && !def_of(*p).passive())
            .filter(|p| fresh || !def_of(*p).toggle);
        let blast = kit
            .blast
            .filter(|_| input.blast)
            .filter(|_| waiting.is_none_or(quick));
        let smash = input
            .smash
            .then_some(PowerId::STRIKE)
            .filter(|_| waiting.is_none_or(|q| q == PowerId::STRIKE));
        let asked = key.or(blast).or(smash);
        let pressing = input.power != 0 || input.blast || input.smash;
        if pressing && !actors[i].f.pressing {
            let target = input.target.and_then(|t| actors.iter().find(|a| a.id == t));
            let about = match target {
                Some(t) => format!(
                    "target {:?} {:.1} m away, {}, {}",
                    t.id,
                    t.body.pos.distance(actors[i].body.pos),
                    if hittable(t) {
                        "can be hit"
                    } else {
                        "can NOT be hit"
                    },
                    if in_sight(solids, actors[i].chest(), t.chest()) {
                        "in sight"
                    } else {
                        "NOT in sight"
                    }
                ),
                None => "no target".into(),
            };
            out.log.push(format!(
                "server {tick}: input power {} blast {} punch {} -> {} ({about}; hero {:?}, energy {:.0}, waiting: {:?})",
                input.power,
                input.blast,
                input.smash,
                asked.map_or("nothing".into(), |p| def_of(p).name.to_string()),
                actors[i].f.status,
                actors[i].f.energy,
                waiting.map(|p| def_of(p).name),
            ));
        }
        actors[i].f.pressing = pressing;
        let held_toggle = kit.has(PowerId(input.power)) && def_of(PowerId(input.power)).toggle;
        if input.power != 0 && key.is_none() && !held_toggle {
            out.log.push(format!(
                "server {tick}: power {} is not in this hero's kit (or passive)",
                input.power
            ));
        }
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
            // Pressed again (or another key) while the last one could not
            // fire: say why now, so pressing again and again is never silent.
            let why = std::mem::take(&mut actors[i].f.waiting);
            // (Not for a recharge: the new press waits for it and fires.)
            let quiet = (tick < actors[i].f.told + ticks(1.0) && actors[i].f.told > 0)
                || matches!(why, fizzle::RECHARGING | fizzle::BUSY);
            if let Some((q, _)) = actors[i].f.queued.filter(|_| why != 0 && !quiet) {
                actors[i].f.told = tick;
                let me = actors[i].id;
                let mut e = event(tick, FxKind::Fizzle, me, me, actors[i].chest());
                e.amount = why;
                e.theme = q.0;
                out.fx.push(e);
            }
            // A power still recharging waits for it, if not for long.
            let ready = actors[i].f.ready[p.0 as usize];
            let wait = if ready > tick && ready - tick <= ticks(RECHARGE_WAIT) {
                ready
            } else {
                0
            };
            actors[i].f.queued = Some((p, (tick + ticks(QUEUE_SECONDS)).max(wait)));
            actors[i].f.waiting = 0;
        }
        let Some((power, until)) = actors[i].f.queued else {
            continue;
        };
        let d = def_of(power);
        let f = d.fields;
        let name = d.name;
        if !actors[i].f.standing() {
            out.log.push(format!(
                "server {tick}: {name} dropped: the hero is {:?}",
                actors[i].f.status
            ));
            actors[i].f.queued = None;
            continue;
        }
        if tick > until {
            out.log.push(format!(
                "server {tick}: {name} lapsed: {}",
                why_words(actors[i].f.waiting)
            ));
            // It lapsed: say why, so a key press never just does nothing.
            actors[i].f.queued = None;
            let why = std::mem::take(&mut actors[i].f.waiting);
            if why != 0 {
                actors[i].f.told = tick;
                let me = actors[i].id;
                let mut e = event(tick, FxKind::Fizzle, me, me, actors[i].chest());
                e.amount = why;
                e.theme = power.0;
                out.fx.push(e);
            }
            continue;
        }
        let delivery = d.delivery();
        let throwing = delivery == Delivery::Grab && actors[i].f.carrying.is_some();
        // A cone goes where the hero aims when nothing is targeted (at a
        // fire, say), so it needs no target either.
        let free_aim = throwing || delivery == Delivery::Cone;
        let aimed = d.aimed() || throwing;
        // The picked target, or else the nearest villain in reach (for a
        // ranged power, one near where the hero aims).
        let pos = actors[i].body.pos;
        let aim = DVec3::new(
            input.aim[0] as f64,
            input.aim[1] as f64,
            input.aim[2] as f64,
        )
        .normalize_or(DVec3::ZERO);
        let range = if throwing { f.range.max(30.0) } else { f.range };
        let mut target = None;
        if aimed {
            target = match actors[i].f.target {
                Some(t) => actors.iter().position(|a| a.id == t),
                None if delivery == Delivery::Melee => {
                    nearest_seen_enemy(actors, solids, actors[i].chest(), range)
                }
                None => {
                    let chest = actors[i].chest();
                    let reach = range.min(25.0);
                    actors
                        .iter()
                        .enumerate()
                        .filter(|(_, a)| hittable(a))
                        // Only what the hero can see: never a villain
                        // behind a wall when one in the open is nearer
                        // the aim.
                        .filter(|(_, a)| in_sight(solids, chest, a.chest()))
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
            let mut why = fizzle::NO_TARGET;
            if let Some(t) = target {
                let in_reach = actors[t].body.pos.distance(pos) <= range;
                // No blow and no power through a wall, a punch included.
                let seen = in_sight(solids, actors[i].chest(), actors[t].chest());
                if !in_reach || !seen {
                    target = None;
                    why = if in_reach {
                        fizzle::NOT_SEEN
                    } else {
                        fizzle::OUT_OF_RANGE
                    };
                }
            }
            if target.is_none() && !free_aim {
                note_wait(&mut actors[i].f, why, name, tick, &mut out.log);
                continue;
            }
            // Bosses and machines shrug off hypnosis.
            if f.charm > 0.0
                && f.damage <= 0.0
                && target.is_some_and(|t| sticks(&actors[t], Hinder::Charm) <= 0.0)
            {
                actors[i].f.queued = None;
                continue;
            }
            // From here the blow is the villain's, whether it lands now or
            // once the power is ready.
            actors[i].f.engaged = target.is_some();
        }
        if delivery == Delivery::Burst && nearest_enemy(actors, pos, f.radius).is_some() {
            actors[i].f.engaged = true;
        }
        // Sidekicks already out are sent at the target: free and at once.
        let my_sidekicks: Vec<usize> = (0..actors.len())
            .filter(|&k| {
                actors[k]
                    .sidekick
                    .is_some_and(|s| s.owner == actors[i].id && s.power == power)
                    && actors[k].f.fighting()
            })
            .collect();
        let commanding = delivery == Delivery::Summon && my_sidekicks.len() >= f.summons as usize;
        // The portal's buttons: where its home or its far side would
        // stand. Where it cannot, it fizzles at once (as the stone's did),
        // costing nothing.
        let gate = match f.lands {
            Lands::GateMark | Lands::GateOpen if delivery == Delivery::Blink => {
                let Some(owner) = actors[i].hero.map(|h| h.0).filter(|_| !actors[i].npc) else {
                    actors[i].f.queued = None;
                    continue;
                };
                let feet = actors[i].body.pos;
                let flat = DVec3::new(actors[i].body.vel.x, 0.0, actors[i].body.vel.z);
                let yaw = crate::gates::yaw_of(aim)
                    .or_else(|| crate::gates::yaw_of(flat))
                    .unwrap_or(0.0);
                let spot = if f.lands == Lands::GateMark {
                    crate::gates::mark_spot(solids, feet, yaw)
                        .map(|s| (s, 1.0))
                        .ok_or(fizzle::NO_ROOM)
                } else {
                    match crate::gates::open_spot(solids, feet, yaw) {
                        None => Err(fizzle::NO_ROOM),
                        Some(s) => gates
                            .can_open(owner, solids, &s)
                            .map(|k| (s, k))
                            .map_err(|e| match e {
                                crate::gates::Refused::NoHome => fizzle::NO_HOME,
                                crate::gates::Refused::NoRoom => fizzle::NO_ROOM,
                            }),
                    }
                };
                match spot {
                    Ok(s) => Some((owner, s)),
                    Err(why) => {
                        out.log
                            .push(format!("server {tick}: {name} fizzled: {}", why_words(why)));
                        actors[i].f.queued = None;
                        actors[i].f.told = tick;
                        let me = actors[i].id;
                        // Where it tried to open (the fizzle's light).
                        let (s, c) = hw_core::det::sin_cos(yaw);
                        let tried = feet
                            + DVec3::new(s, 0.0, c) * crate::gates::OPEN_AHEAD
                            + DVec3::Y * 1.3;
                        let mut e = event(tick, FxKind::Fizzle, me, me, tried);
                        e.amount = why;
                        e.theme = power.0;
                        out.fx.push(e);
                        continue;
                    }
                }
            }
            _ => None,
        };
        // Where a move lands, and which rescues a bubble goes to: none,
        // and the power waits (and then lapses) without costing anything.
        let blink = if delivery == Delivery::Blink && gate.is_none() {
            let to = match f.lands {
                Lands::Home => home,
                _ => {
                    // At a villain picked in reach (a dash, a leap), else
                    // where the hero aims.
                    let foe = actors[i]
                        .f
                        .target
                        .and_then(|t| actors.iter().position(|a| a.id == t))
                        .filter(|_| f.damage > 0.0)
                        .filter(|&t| actors[t].body.pos.distance(pos) <= f.range + 1.0)
                        .filter(|&t| in_sight(solids, actors[i].chest(), actors[t].chest()));
                    match foe {
                        Some(t) => {
                            let to = actors[t].body.pos - pos;
                            let way = DVec3::new(to.x, 0.0, to.z);
                            let len = way.length();
                            teleport_within(
                                solids,
                                pos,
                                to.normalize_or(DVec3::X),
                                (len - 1.2).max(1.0) + 0.8,
                            )
                            .or(Some(actors[t].body.pos - way.normalize_or(DVec3::X) * 1.2))
                        }
                        None => teleport_within(solids, pos, aim, f.range),
                    }
                }
            };
            match to {
                Some(p) => Some(p),
                None => {
                    note_wait(&mut actors[i].f, fizzle::NO_SPOT, name, tick, &mut out.log);
                    continue;
                }
            }
        } else {
            None
        };
        let rescues = if delivery == Delivery::Rescue {
            let found = nearest_rescues(
                rescue_points,
                actors[i].chest(),
                aim,
                f.range,
                f.rescues.max(1) as usize,
            );
            if found.is_empty() {
                note_wait(&mut actors[i].f, fizzle::NO_SPOT, name, tick, &mut out.log);
                continue;
            }
            found
        } else {
            Vec::new()
        };
        let me = &actors[i].f;
        // Lifting paid for the throw; a lift waits for the last throw's recharge.
        let cost = if throwing || commanding {
            0.0
        } else {
            powers::energy(d)
        };
        let ready = throwing || commanding || tick >= me.ready[power.0 as usize];
        if tick < me.busy_until || !ready || me.energy < cost {
            let why = if !ready {
                fizzle::RECHARGING
            } else if me.energy < cost {
                fizzle::ENERGY
            } else {
                fizzle::BUSY
            };
            note_wait(&mut actors[i].f, why, name, tick, &mut out.log);
            continue;
        }
        let me_id = actors[i].id;
        let element = kit.element.code();
        actors[i].f.queued = None;
        out.log.push(format!(
            "server {tick}: {name} fired at {:?}",
            target.map(|t| actors[t].id)
        ));
        let me = &mut actors[i].f;
        me.energy -= cost;
        me.busy_until = tick + ticks(ACTION_LOCK);
        if (delivery != Delivery::Grab || throwing) && !commanding {
            me.ready[power.0 as usize] = tick + ticks(powers::recharge(d) / me.haste());
        }
        let level = me.level;
        let damage = f.damage * level_scale(level) * me.outgoing(tick);
        let event_of = |kind, to: NetId, at: DVec3| {
            let mut e = event(tick, kind, me_id, to, at);
            e.theme = power.0;
            e.element = element;
            e
        };
        match delivery {
            Delivery::Melee | Delivery::Ranged => {
                let Some(t) = target else {
                    continue;
                };
                actors[i].f.target = Some(actors[t].id);
                // A bolt or a punch can meet a shield; a hold's field
                // closes round it.
                let shot = d.base != Power::Hold;
                for _ in 0..f.repeats.max(1) {
                    if !hittable(&actors[t]) {
                        break;
                    }
                    blow(actors, i, t, damage, power, pos, shot, tick, out);
                }
            }
            Delivery::Line => {
                let Some(t) = target else {
                    continue;
                };
                actors[i].f.target = Some(actors[t].id);
                let from = actors[i].chest();
                let dir = (actors[t].chest() - from).normalize_or(DVec3::X);
                // The beam runs on to its full length or the first wall.
                let mut len = f.range;
                let steps = (f.range / 0.5) as usize;
                for s in 1..=steps {
                    let p = from + dir * (s as f64 * 0.5);
                    if solids.solid_at(p) {
                        len = s as f64 * 0.5;
                        break;
                    }
                }
                let mut struck: Vec<(usize, f64)> = (0..actors.len())
                    .filter(|&k| hittable(&actors[k]))
                    .filter_map(|k| {
                        let to = actors[k].chest() - from;
                        let along = to.dot(dir);
                        let off = (to - dir * along).length();
                        (along > 0.0 && along <= len && off <= f.radius).then_some((k, along))
                    })
                    .collect();
                struck.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
                for (k, _) in struck {
                    blow(actors, i, k, damage, power, pos, false, tick, out);
                }
                actors[i].f.last_combat = tick;
                out.fx.push(event_of(FxKind::Beam, me_id, from + dir * len));
                if f.wreck > 0.0 && !f.spare {
                    let blast = crate::destruction::hero_power(level).blast;
                    let mid = from + dir * (len * 0.5);
                    out.city.push((mid, 2.0, blast * f.wreck));
                }
            }
            Delivery::Cone => {
                let from = actors[i].chest();
                let dir = match target {
                    Some(t) => (actors[t].chest() - from).normalize_or(DVec3::X),
                    None if aim != DVec3::ZERO => aim,
                    None => continue,
                };
                if let Some(t) = target {
                    actors[i].f.target = Some(actors[t].id);
                }
                // The cone runs to its full length or the first wall.
                let mut len = f.range;
                for s in 1..=(f.range / 0.5) as usize {
                    if solids.solid_at(from + dir * (s as f64 * 0.5)) {
                        len = s as f64 * 0.5;
                        break;
                    }
                }
                let mut caught: Vec<(usize, f64)> = (0..actors.len())
                    .filter(|&k| hittable(&actors[k]))
                    .filter_map(|k| {
                        let to = actors[k].chest() - from;
                        let along = to.dot(dir);
                        let off = (to - dir * along).length();
                        let wide = 0.8 + along * f.radius;
                        (along > 0.0 && along <= len + 0.6 && off <= wide).then_some((k, along))
                    })
                    .filter(|&(k, _)| clear_line(solids, from, actors[k].chest()))
                    .collect();
                caught.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
                for (k, _) in caught {
                    blow(actors, i, k, damage, power, pos, false, tick, out);
                }
                actors[i].f.last_combat = tick;
                // A freezing cone puts fires out.
                if f.freeze > 0.0 {
                    out.cold.push((from, dir, len));
                }
                out.fx.push(event_of(look(d), me_id, from + dir * len));
            }
            Delivery::Burst => {
                let chest = actors[i].chest();
                let caught = burst(
                    actors,
                    i,
                    Burst {
                        centre: pos,
                        sight: Some(chest),
                        damage,
                        power,
                    },
                    solids,
                    tick,
                    out,
                );
                actors[i].f.last_combat = tick;
                let mut e = event_of(look(d), me_id, pos);
                e.amount = caught;
                out.fx.push(e);
                if f.heal > 0.0 || f.lasts > 0.0 {
                    allies(actors, i, power, pos, level, solids, tick, out);
                }
                if f.wreck > 0.0 && !f.spare {
                    // The street round the hero takes it too.
                    let blast = crate::destruction::hero_power(level).blast;
                    out.city.push((pos + DVec3::Y * 0.5, 4.0, blast * f.wreck));
                }
            }
            Delivery::Sky => {
                let Some(t) = target else {
                    continue;
                };
                actors[i].f.target = Some(actors[t].id);
                let at = actors[t].body.pos;
                for k in 0..f.repeats.max(1) {
                    // Later strikes land round the first, one a second.
                    let a = k as f64 * 2.4;
                    let off = if k == 0 {
                        DVec3::ZERO
                    } else {
                        DVec3::new(a.cos(), 0.0, a.sin()) * f.radius * 0.6
                    };
                    out.sky.push(SkyStrike {
                        land: tick + ticks(f.delay + k as f64),
                        from: me_id,
                        at: at + off,
                        damage,
                        power,
                        level,
                    });
                }
                actors[i].f.last_combat = tick;
                out.fx.push(event_of(FxKind::MeteorCall, actors[t].id, at));
            }
            Delivery::Allies => {
                allies(actors, i, power, pos, level, solids, tick, out);
            }
            Delivery::Grab if !throwing => {
                actors[i].f.lifted_with = power;
                out.lifts.push(i)
            }
            Delivery::Grab => {
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
                        power,
                    });
                }
                actors[i].f.last_combat = tick;
                out.fx
                    .push(event_of(FxKind::Throw, tid.unwrap_or(me_id), from));
            }
            Delivery::Blink if gate.is_some() => {
                let Some((owner, (side, size))) = gate else {
                    continue;
                };
                let turn = crate::gates::yaw_turns;
                let shut = |s: &crate::gates::Side, out: &mut Out| {
                    let mut e = event_of(FxKind::GateShut, me_id, s.at);
                    e.amount = turn(s.yaw);
                    out.fx.push(e);
                };
                if f.lands == Lands::GateMark {
                    if let Some(old) = gates.mark(owner, side) {
                        shut(&old.home, out);
                        shut(&old.far, out);
                    }
                    let mut e = event_of(FxKind::GateMark, me_id, side.at);
                    e.amount = turn(side.yaw);
                    out.fx.push(e);
                } else if let Some((_, closed)) = gates.open(owner, side, size, tick) {
                    if let Some(old) = closed {
                        shut(&old, out);
                    }
                    let mut e = event_of(FxKind::GateOpen, me_id, side.at);
                    e.amount = turn(side.yaw);
                    out.fx.push(e);
                }
            }
            Delivery::Blink => {
                let Some(to) = blink else {
                    continue;
                };
                let from = actors[i].body.pos;
                out.fx.push(event_of(FxKind::TeleportOut, me_id, from));
                let b = &mut actors[i].body;
                b.pos = to;
                // Arrives at rest (falls if it is in the air).
                b.vel = DVec3::ZERO;
                b.grounded = false;
                out.fx.push(event_of(FxKind::TeleportIn, me_id, to));
                match f.lands {
                    Lands::Path if f.damage > 0.0 => {
                        // Everyone along the way.
                        let way = to - from;
                        let len = way.length().max(0.01);
                        let dir = way / len;
                        let mut hit_on: Vec<(usize, f64)> = (0..actors.len())
                            .filter(|&k| hittable(&actors[k]))
                            .filter_map(|k| {
                                let rel = actors[k].body.pos - from;
                                let along = rel.dot(dir);
                                let off = (rel - dir * along).length();
                                (along > -0.5 && along <= len + 1.5 && off <= 1.8)
                                    .then_some((k, along))
                            })
                            .collect();
                        hit_on.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
                        for (k, _) in hit_on {
                            blow(actors, i, k, damage, power, from, false, tick, out);
                        }
                        actors[i].f.last_combat = tick;
                    }
                    Lands::Arrival | Lands::Start | Lands::Target if f.damage > 0.0 => {
                        let centre = if f.lands == Lands::Start { from } else { to };
                        let caught = burst(
                            actors,
                            i,
                            Burst {
                                centre,
                                sight: Some(centre + DVec3::Y),
                                damage,
                                power,
                            },
                            solids,
                            tick,
                            out,
                        );
                        actors[i].f.last_combat = tick;
                        let mut e = event_of(FxKind::Shockwave, me_id, centre);
                        e.amount = caught;
                        out.fx.push(e);
                        if f.wreck > 0.0 && !f.spare {
                            let blast = crate::destruction::hero_power(level).blast;
                            out.city
                                .push((centre + DVec3::Y * 0.5, 3.0, blast * f.wreck));
                        }
                    }
                    _ => {}
                }
                if f.shield > 0.0 || f.heal > 0.0 {
                    allies(actors, i, power, to, level, solids, tick, out);
                }
            }
            Delivery::Rescue => {
                for at in rescues {
                    out.rescues.push(Rescue {
                        hero: me_id,
                        level,
                        at,
                    });
                }
            }
            Delivery::Summon => {
                // The villain to go for: the hero's target, else the
                // nearest one near the hero.
                let foe = target
                    .or_else(|| {
                        actors[i]
                            .f
                            .target
                            .and_then(|t| actors.iter().position(|a| a.id == t))
                    })
                    .filter(|&t| hittable(&actors[t]))
                    .or_else(|| nearest_enemy(actors, pos, 20.0))
                    .map(|t| actors[t].id);
                if commanding {
                    for &k in &my_sidekicks {
                        if let Some(f) = foe {
                            actors[k].f.target = Some(f);
                        }
                        out.fx
                            .push(event_of(FxKind::SidekickCall, actors[k].id, pos));
                    }
                } else {
                    let Some((player, _)) = actors[i].hero else {
                        continue;
                    };
                    for n in 0..f.summons.max(1) {
                        // At the hero's side, a step back.
                        let back = DVec3::new(-aim.x, 0.0, -aim.z).normalize_or(DVec3::Z);
                        let side =
                            DVec3::new(back.z, 0.0, -back.x) * if n % 2 == 0 { 1.2 } else { -1.2 };
                        let mut at = pos + back * 1.4 + side;
                        if solids.solid_at(at + DVec3::Y * 0.6) {
                            at = pos;
                        }
                        out.summons.push(Summon {
                            owner: me_id,
                            player,
                            power,
                            index: n,
                            at,
                            level,
                        });
                        out.fx.push(event_of(FxKind::SidekickCall, me_id, at));
                    }
                }
            }
            Delivery::Passive => {
                // A toggle: switched on or off.
                if d.toggle {
                    let on = actors[i].f.kit.flip(power);
                    let mut e = event_of(FxKind::Buff, me_id, actors[i].chest());
                    e.knock = on;
                    out.fx.push(e);
                    out.log.push(format!(
                        "server {tick}: {name} switched {}",
                        if on { "on" } else { "off" }
                    ));
                }
            }
        }
    }
}

/// Minions freeze solid; lieutenants, bosses and machines only slow down.
pub fn freezable(k: EnemyKind) -> bool {
    enemy_spec(k).rank == "Minion" && !k.heavy()
}

/// Bosses and machines cannot be hypnotised.
pub fn charmable(k: EnemyKind) -> bool {
    matches!(enemy_spec(k).rank, "Minion" | "Lieutenant") && !k.heavy()
}

/// Where a teleport from feet at `from` along `aim` lands: as far as
/// [`TELEPORT_RANGE`], stopping short of the first wall, roof or ground,
/// and only where a person fits (never inside anything). None when there
/// is no room even a few metres on.
pub fn teleport_to(solids: &Obstacles, from: DVec3, aim: DVec3) -> Option<DVec3> {
    teleport_within(solids, from, aim, TELEPORT_RANGE)
}

/// [`teleport_to`] reaching `range` metres at most.
pub fn teleport_within(solids: &Obstacles, from: DVec3, aim: DVec3, range: f64) -> Option<DVec3> {
    if aim == DVec3::ZERO || range < 1.0 {
        return None;
    }
    let eye = from + DVec3::Y * (BODY_HEIGHT * 0.8);
    let step = 0.5;
    let mut reach = range;
    for s in 1..=(range / step) as usize {
        let p = eye + aim * (s as f64 * step);
        let ground = solids.ground_for(DVec2::new(p.x, p.z), p.y);
        if solids.solid_at(p) || p.y < ground + 0.2 {
            reach = s as f64 * step;
            break;
        }
    }
    // Back off from what stopped it until a body fits, feet on whatever is
    // underneath if that is near.
    let mut d = reach - 0.8;
    while d >= 3.0f64.min(range * 0.5) {
        let p = eye + aim * d;
        let flat = DVec2::new(p.x, p.z);
        let floor = solids.floor_under(flat, p.y + 0.4);
        let feet = if p.y - floor < BODY_HEIGHT * 0.8 {
            floor
        } else {
            p.y - BODY_HEIGHT * 0.8
        };
        let fits = [0.15, 0.6, 1.1, BODY_HEIGHT - 0.1]
            .iter()
            .all(|h| !solids.solid_at(DVec3::new(p.x, feet + h, p.z)))
            && feet >= solids.ground_for(flat, feet + 0.5) - 0.05;
        if fits {
            return Some(DVec3::new(p.x, feet + 0.02, p.z));
        }
        d -= 0.5;
    }
    None
}

/// The trapped or hurt people rescue bubbles go to: within `range`, the
/// `n` nearest where the hero aims.
fn nearest_rescues(points: &[DVec3], chest: DVec3, aim: DVec3, range: f64, n: usize) -> Vec<DVec3> {
    let off = |p: DVec3| {
        let to = p - chest;
        let along = to.dot(aim).max(0.0);
        (to - aim * along).length() + to.length() * 0.05
    };
    let mut near: Vec<DVec3> = points
        .iter()
        .copied()
        .filter(|p| p.distance(chest) <= range)
        .collect();
    near.sort_by(|a, b| off(*a).total_cmp(&off(*b)));
    near.truncate(n);
    near
}

/// A blast all round a point (shockwave, nova, a meteor landing).
/// A hard super-jump landing's stomp at full strength (before the level
/// scale) and its reach: from [`STOMP_REACH`] at a [`crate::HARD_LANDING`]
/// up to twice that from the top of a level-20 leap.
pub const STOMP_DAMAGE: f32 = 14.0;
/// The Super Jump power.
const SUPER_JUMP: PowerId = PowerId(2);
pub const STOMP_REACH: f64 = 3.5;

/// How hard a landing at `fall` m/s is: 0 at [`crate::HARD_LANDING`], 1
/// at the bottom of a full level-20 leap.
pub fn landing_force(fall: f32) -> f32 {
    let top = crate::super_jump_speed(20) as f32;
    let hard = crate::HARD_LANDING as f32;
    ((fall - hard) / (top - hard)).clamp(0.0, 1.0)
}

/// Super jumps: a take-off shows (dust, a whoosh), and a hard landing
/// stomps: villains near the hero's feet are hit and knocked over, more
/// and further the higher the leap came from.
fn leaps(actors: &mut [Actor], solids: &Obstacles, tick: u64, out: &mut Out) {
    for i in 0..actors.len() {
        if actors[i].hero.is_none() {
            continue;
        }
        let feet = actors[i].body.pos;
        let me = actors[i].id;
        if std::mem::take(&mut actors[i].f.leapt) {
            out.fx.push(event(tick, FxKind::Launch, me, me, feet));
        }
        let fall = std::mem::take(&mut actors[i].f.landed);
        if fall <= 0.0 {
            continue;
        }
        let force = landing_force(fall);
        let reach = STOMP_REACH * (1.0 + force as f64);
        let damage = STOMP_DAMAGE * (0.5 + force) * level_scale(actors[i].f.level);
        let standing = actors[i].f.fighting();
        let mut caught = 0u16;
        if standing {
            let mut near: Vec<(usize, f64)> = (0..actors.len())
                .filter(|&k| k != i && hittable(&actors[k]))
                .map(|k| (k, actors[k].body.pos.distance(feet)))
                .filter(|&(k, d)| {
                    d <= reach && clear_line(solids, feet + DVec3::Y, actors[k].chest())
                })
                .collect();
            near.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
            for (k, _) in near {
                let (knock, defeat, dealt) = hit(actors, i, k, damage, true, tick);
                let mut e = event(tick, FxKind::Landing, me, actors[k].id, actors[k].chest());
                e.amount = dealt.round() as u16;
                e.knock = knock;
                e.defeat = defeat;
                e.theme = SUPER_JUMP.0;
                out.fx.push(e);
                caught += 1;
            }
            if caught > 0 {
                actors[i].f.last_combat = tick;
            }
        }
        // The hero's own landing: how hard (tenths of m/s) and whom it caught.
        let mut e = event(tick, FxKind::Landing, me, me, feet);
        e.amount = (fall * 10.0).round() as u16;
        e.knock = caught > 0;
        out.fx.push(e);
    }
}

struct Burst {
    centre: DVec3,
    /// Where walls are checked from (a blast does not go through them);
    /// None for a blast from the sky, which nothing shelters from.
    sight: Option<DVec3>,
    damage: f32,
    /// The power: its radius, what it does besides, its look.
    power: PowerId,
}

/// Hits every villain in `b`'s area that the blast can reach; returns how
/// many.
fn burst(
    actors: &mut [Actor],
    i: usize,
    b: Burst,
    solids: &Obstacles,
    tick: u64,
    out: &mut Out,
) -> u16 {
    let area = def_of(b.power).fields.radius;
    let mut caught = 0;
    for k in 0..actors.len() {
        if !hittable(&actors[k])
            || actors[k].body.pos.distance(b.centre) > area
            || b.sight
                .is_some_and(|s| !clear_line(solids, s, actors[k].chest()))
        {
            continue;
        }
        blow(actors, i, k, b.damage, b.power, b.centre, false, tick, out);
        caught += 1;
    }
    caught
}

/// The share of their own ward a warding hero gets.
pub const OWN_WARD: f32 = 0.5;

/// Who stands in a friend's aura this tick: every hero and sidekick within
/// [`powers::AURA_RADIUS`] of a standing hero whose powers give one (their
/// own too) hits harder; a ward makes friends take less (the warding hero
/// [`OWN_WARD`] of it).
fn rally(actors: &mut [Actor]) {
    let auras: Vec<(DVec3, f32, f32, NetId)> = actors
        .iter()
        .filter(|a| a.hero.is_some() && a.f.fighting())
        .filter(|a| a.f.kit.passive.aura > 0.0 || a.f.kit.passive.ward > 0.0)
        .map(|a| (a.body.pos, a.f.kit.passive.aura, a.f.kit.passive.ward, a.id))
        .collect();
    for a in actors.iter_mut() {
        a.f.rally = 0.0;
        a.f.warded = 0.0;
        if (a.hero.is_none() && a.sidekick.is_none()) || !a.f.fighting() {
            a.f.rallied = false;
            continue;
        }
        for &(p, aura, ward, by) in &auras {
            if p.distance(a.body.pos) <= powers::AURA_RADIUS {
                a.f.rally = a.f.rally.max(aura);
                // The one warding takes half the good of it.
                let share = if by == a.id { OWN_WARD } else { 1.0 };
                a.f.warded = a.f.warded.max(ward * share);
            }
        }
        a.f.rallied = a.f.rally > 0.0;
    }
}

/// Strikes from the sky that land this tick: a blast where they come
/// down, and a crater in the street.
fn land_sky(
    actors: &mut [Actor],
    sky: &mut Vec<SkyStrike>,
    solids: &Obstacles,
    tick: u64,
    out: &mut Out,
) {
    let due: Vec<SkyStrike> = sky.iter().copied().filter(|m| m.land <= tick).collect();
    sky.retain(|m| m.land > tick);
    for m in due {
        let Some(i) = actors.iter().position(|a| a.id == m.from) else {
            continue;
        };
        let f = def_of(m.power).fields;
        let caught = burst(
            actors,
            i,
            Burst {
                centre: m.at,
                sight: None,
                damage: m.damage,
                power: m.power,
            },
            solids,
            tick,
            out,
        );
        actors[i].f.last_combat = tick;
        let mut e = event(tick, FxKind::Meteor, m.from, m.from, m.at);
        e.theme = m.power.0;
        e.element = actors[i].f.kit.element.code();
        e.amount = caught;
        out.fx.push(e);
        if f.wreck > 0.0 && !f.spare {
            let blast = crate::destruction::hero_power(m.level).blast;
            out.city.push((m.at + DVec3::Y * 0.5, 3.5, blast * f.wreck));
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
        let at = actors[i].body.pos;
        blow(actors, i, t, th.damage, th.power, at, false, tick, out);
    }
}

/// Burns deal their damage, a step at a time.
fn burns(actors: &mut [Actor], tick: u64, out: &mut Out) {
    let step = ticks(BURN_STEP);
    if !tick.is_multiple_of(step) {
        return;
    }
    for t in 0..actors.len() {
        if !actors[t].f.burning(tick) || !actors[t].f.fighting() {
            continue;
        }
        let damage = actors[t].f.burn * BURN_STEP as f32;
        let by = actors[t].f.burn_by;
        let from = by.and_then(|b| actors.iter().position(|a| a.id == b));
        let (defeat, dealt) = match from {
            Some(i) => {
                let (_, defeat, dealt) = strike(actors, i, t, damage, false, tick, false);
                (defeat, dealt)
            }
            None => {
                let f = &mut actors[t].f;
                f.health = (f.health - damage).max(1.0);
                (false, damage)
            }
        };
        let mut e = event(
            tick,
            FxKind::Burn,
            by.unwrap_or(actors[t].id),
            actors[t].id,
            actors[t].chest(),
        );
        e.amount = dealt.round() as u16;
        e.defeat = defeat;
        e.element = from.map_or(0, |i| actors[i].f.kit.element.code());
        out.fx.push(e);
    }
}

/// Karma for every villain beaten this tick, to each hero near enough to
/// have helped: tougher ones pay more (and a bounty hunter more again).
fn pay_out(actors: &[Actor], out: &mut Out) {
    for v in actors {
        let (Some(by), Some(en)) = (v.beaten_by, v.enemy) else {
            continue;
        };
        // Beaten by one of the town's heroes: nobody's karma.
        if actors.iter().any(|a| a.id == by && a.npc) {
            continue;
        }
        let base = enemy_spec(en.kind).karma;
        for h in actors.iter().filter(|a| a.hero.is_some()) {
            if h.body.pos.distance(v.body.pos) > REWARD_RADIUS {
                continue;
            }
            let karma = crate::progress::defeat_karma(base, v.f.level, h.f.level);
            let bounty = 1.0 + h.f.kit.passive.bounty;
            out.rewards.push(Reward {
                hero: h.id,
                karma: (karma as f32 * bounty).round() as u32,
            });
            // Parts of a machine are not people of the faction.
            if en.kind != EnemyKind::WeakPoint {
                out.deeds.push(Deed {
                    hero: h.id,
                    beaten: crate::factions::of_enemy(en.kind),
                });
            }
        }
    }
}

/// How far ahead a villain looks for its way (m).
const LOOK_AHEAD: f64 = 40.0;

/// Which way a villain at `here` walks to reach `goal`: straight when the
/// way is clear, else along a path round the walls (found again now and
/// then, as the goal moves).
fn heading(
    paths: &mut BTreeMap<NetId, Path>,
    solids: &Obstacles,
    floors: Option<&crate::floornav::Floors>,
    id: NetId,
    tick: u64,
    here: DVec3,
    goal: DVec3,
) -> DVec2 {
    let flat = |a: DVec3, b: DVec3| DVec2::new(b.x - a.x, b.z - a.z);
    let direct = flat(here, goal).normalize_or_zero();
    // In (or into, or out of) a building with a route map: through its
    // doorways and up its stairs.
    if let Some(fl) =
        floors.filter(|f| f.building_at(here).is_some() || f.building_at(goal).is_some())
    {
        let stale = paths
            .get(&id)
            .is_none_or(|p| tick >= p.made + ticks(1.5) || p.goal.distance(goal) > 3.0);
        if stale {
            match fl.way(solids, here, goal) {
                Some(points) if !points.is_empty() => {
                    paths.insert(
                        id,
                        Path {
                            made: tick,
                            goal,
                            points,
                        },
                    );
                }
                _ => {
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
        return flat(here, p.points[0]).normalize_or(direct);
    }
    // Far away (a villain flung across town walking home): find the way
    // over the next stretch only, so a long walk costs no more each tick
    // than a short one.
    let far = flat(here, goal).length();
    let goal = if far > LOOK_AHEAD {
        here + (goal - here) * (LOOK_AHEAD / far)
    } else {
        goal
    };
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
    floors: Option<&crate::floornav::Floors>,
    tick: u64,
    fx: &mut Vec<Fx>,
    paths: &mut BTreeMap<NetId, Path>,
    bombs: &mut Vec<Bomb>,
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
        let ranged = en.kind.ranged();
        let mut input = InputState::default();
        let getting_up = tick < actors[i].f.busy_until;
        let held = actors[i].f.held(tick) || actors[i].f.frozen(tick);
        let charmed = actors[i].f.charmed(tick);
        let slowed = actors[i].f.slowed(tick) as f64;
        if slowed > 0.0 {
            spec.windup *= 1.0 + slowed * (CHILL_WINDUP - 1.0) / CHILL_SLOW;
            spec.speed *= 1.0 - slowed;
        }
        let lost = actors[i].f.confused(tick) || actors[i].f.feared(tick);
        if actors[i].f.standing() && !getting_up && !held && lost {
            // Confused: wanders this way and that; afraid: runs.
            en.windup = None;
            let here = actors[i].body.pos;
            let go = if actors[i].f.feared(tick) {
                let from = actors[i].f.fear_from;
                DVec2::new(here.x - from.x, here.z - from.z).normalize_or(DVec2::X)
            } else {
                let a = roll(tick / 45, actors[i].id) * std::f64::consts::TAU;
                DVec2::new(a.cos(), a.sin()) * 0.35
            };
            input.move_x = go.x as f32;
            input.move_z = go.y as f32;
        } else if actors[i].f.standing() && !getting_up && !held {
            // Who to fight: the foe while it stands and is not led too far
            // from home, else the nearest standing hero it notices. A
            // hypnotised villain fights its own kind instead, and its own
            // kind fights it.
            let leash = 45.0;
            let foe_ok = |a: &Actor| {
                if charmed {
                    hittable(a)
                } else {
                    ((a.hero.is_some() && !a.f.hidden(tick))
                        || a.npc
                        || a.sidekick.is_some()
                        || a.f.charmed(tick))
                        && a.f.standing()
                }
            };
            let mut foe = actors[i]
                .f
                .target
                .and_then(|t| actors.iter().position(|a| a.id == t))
                .filter(|&j| {
                    foe_ok(&actors[j]) && (charmed || actors[j].body.pos.distance(en.home) < leash)
                });
            if foe.is_none() && charmed {
                let me = actors[i].chest();
                foe = (0..actors.len())
                    .filter(|&j| j != i && hittable(&actors[j]))
                    .map(|j| (j, actors[j].body.pos.distance(actors[i].body.pos)))
                    .filter(|&(j, d)| d < CHARM_REACH && in_sight(solids, me, actors[j].chest()))
                    .min_by(|x, y| x.1.total_cmp(&y.1).then(x.0.cmp(&y.0)))
                    .map(|(j, _)| j);
            } else if foe.is_none() {
                let me = actors[i].chest();
                let mut noticed: Vec<(usize, f64)> = heroes
                    .iter()
                    .copied()
                    .filter(|&j| foe_ok(&actors[j]))
                    .map(|j| (j, actors[j].body.pos.distance(actors[i].body.pos)))
                    .filter(|(j, d)| {
                        // A lookout hears the street below without seeing it.
                        *d < spec.aggro && (en.perch || in_sight(solids, me, actors[*j].chest()))
                    })
                    .collect();
                noticed.sort_by(|x, y| x.1.total_cmp(&y.1));
                // What it does about each depends on how its faction regards
                // that hero: the nearest it would fight, else a warning.
                let faction = factions::of_enemy(en.kind);
                let mut warn = None;
                for (j, d) in noticed {
                    let hero = &actors[j];
                    let reaction = factions::react(factions::Encounter {
                        attitude: factions::Attitude::of(factions::toward_hero(
                            faction,
                            &hero.regard,
                        )),
                        distance: d,
                        turf: en.turf,
                        crime: en.provoke,
                        trespass: factions::TRESPASS * if en.perch { PERCH_RANGE } else { 1.0 },
                        warned: en
                            .warned
                            .filter(|(h, _)| *h == hero.id)
                            .map(|(_, t)| tick.saturating_sub(t) as f64 * DT),
                    });
                    match reaction {
                        factions::Reaction::Attack => {
                            foe = Some(j);
                            break;
                        }
                        factions::Reaction::Warn if warn.is_none() => warn = Some(j),
                        _ => {}
                    }
                }
                if let (None, Some(j)) = (foe, warn) {
                    // One shout for the whole group.
                    let (spot, hero_id) = (en.spot, actors[j].id);
                    en.warned = Some((hero_id, tick));
                    for a in actors.iter_mut() {
                        if let Some(e) = a.enemy.as_mut().filter(|e| e.spot == spot) {
                            e.warned = Some((hero_id, tick));
                        }
                    }
                    fx.push(event(tick, FxKind::Warn, actors[i].id, hero_id, me));
                }
                if let Some(j) = foe.filter(|&j| !charmed && !actors[j].f.charmed(tick)) {
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
                        // Walking back to its corner; making off at a jog.
                        let pace = if en.leaving { 0.62 } else { 0.3 };
                        let d = heading(paths, solids, floors, id, tick, here, en.home) * pace;
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
                    let seen = in_sight(solids, actors[i].chest(), actors[j].chest());
                    if en.kind == EnemyKind::Doctor {
                        doctor_storm(actors, i, &mut en, tick, dist, fx);
                    }
                    if let Some((at, who)) = en.windup {
                        // The blow lands now, if the foe is still there.
                        if tick >= at {
                            en.windup = None;
                            let target = actors.iter().position(|a| a.id == who);
                            if let Some(t) = target {
                                let d = actors[t].body.pos.distance(actors[i].body.pos);
                                // A foe who stepped behind a wall during
                                // the windup is missed, punch or ray.
                                let lands = actors[t].f.standing()
                                    && d <= spec.reach + 0.6
                                    && in_sight(solids, actors[i].chest(), actors[t].chest());
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
                                    let (_, defeat, dealt) = hit(actors, i, t, dmg, false, tick);
                                    e.amount = dealt.round() as u16;
                                    e.defeat = defeat;
                                }
                                fx.push(e);
                            }
                        }
                    } else {
                        let (near, far) = if en.kind.back_row() {
                            (7.0, 16.0)
                        } else {
                            (0.0, 1.6)
                        };
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
                            go = heading(paths, solids, floors, id, tick, here, there) * spec.speed;
                        } else if dist < near {
                            go = -dir * spec.speed * 0.8;
                        }
                        let can_attack = dist <= spec.reach
                            && seen
                            && tick >= en.next_attack
                            && !actors[i].f.disarmed(tick);
                        // An orderly patches up its friends now and then
                        // (its own clock), and shoots in between.
                        if en.kind == EnemyKind::Orderly
                            && tick >= en.next_trick
                            && mend_friend(actors, i, solids, tick, fx)
                        {
                            en.next_trick = tick + ticks(ORDERLY_EVERY);
                            en.next_attack = en.next_attack.max(tick + ticks(1.0));
                        } else if can_attack && en.kind == EnemyKind::Grenadier {
                            // A bomb lobbed at where the foe stands now: it
                            // bursts there a moment later.
                            let at = actors[j].body.pos;
                            bombs.push(Bomb {
                                land: tick + ticks(BOMB_SECONDS),
                                from: id,
                                at,
                                damage: spec.damage * level_scale(actors[i].f.level),
                            });
                            en.next_attack = tick + ticks(spec.attack_every);
                            let mut e = event(tick, FxKind::Lob, id, actors[j].id, at);
                            e.amount = (BOMB_SECONDS * 1000.0) as u16;
                            fx.push(e);
                            go = DVec2::ZERO;
                        } else if can_attack {
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

/// Does the blow on `t` from `from` (a position) hit its shield? Only a
/// standing shield bearer's, facing its foe at `foe`, and only blows from
/// afar that come from in front.
fn on_shield(t: &Actor, foe: Option<DVec3>, from: DVec3, tick: u64) -> bool {
    let Some(foe) = foe else {
        return false;
    };
    if t.enemy.is_none_or(|e| e.kind != EnemyKind::Bulwark)
        || !t.f.standing()
        || t.f.held(tick)
        || t.f.frozen(tick)
    {
        return false;
    }
    let flat = |p: DVec3| DVec2::new(p.x - t.body.pos.x, p.z - t.body.pos.z);
    let (facing, towards) = (flat(foe), flat(from));
    if towards.length() <= SHIELD_REACH || facing.length() < 1e-6 {
        return false;
    }
    facing.normalize().dot(towards.normalize()) >= SHIELD_ARC.to_radians().cos()
}

/// Bombs that burst this tick catch everyone near (heroes, their
/// sidekicks, Hero Hall's heroes) with a clear line to the burst.
fn land_bombs(
    actors: &mut [Actor],
    bombs: &mut Vec<Bomb>,
    solids: &Obstacles,
    tick: u64,
    out: &mut Out,
) {
    let due: Vec<Bomb> = bombs.iter().copied().filter(|b| b.land <= tick).collect();
    bombs.retain(|b| b.land > tick);
    for b in due {
        out.fx.push(event(tick, FxKind::Bomb, b.from, b.from, b.at));
        // A thrower already beaten and gone: the bomb still bursts, harmless.
        let Some(i) = actors.iter().position(|a| a.id == b.from) else {
            continue;
        };
        let centre = b.at + DVec3::Y * 0.5;
        for k in 0..actors.len() {
            let a = &actors[k];
            let foe = a.hero.is_some() || a.npc || a.sidekick.is_some();
            if !foe || !a.f.standing() || a.body.pos.distance(b.at) > BOMB_AREA {
                continue;
            }
            if !clear_line(solids, centre, a.chest()) {
                continue;
            }
            let (knock, defeat, dealt) = hit(actors, i, k, b.damage, false, tick);
            let mut e = event(tick, FxKind::Bomb, b.from, actors[k].id, actors[k].chest());
            e.amount = dealt.round() as u16;
            e.knock = knock;
            e.defeat = defeat;
            out.fx.push(e);
        }
    }
}

/// A lab orderly's turn: the most hurt of its group in reach and in
/// sight gets patched up. Returns whether it mended anyone.
fn mend_friend(
    actors: &mut [Actor],
    i: usize,
    solids: &Obstacles,
    tick: u64,
    fx: &mut Vec<Fx>,
) -> bool {
    let Some(en) = actors[i].enemy else {
        return false;
    };
    let me = actors[i].chest();
    let hurt = (0..actors.len())
        .filter(|&k| k != i)
        .filter(|&k| {
            let a = &actors[k];
            a.enemy.is_some_and(|e| e.spot == en.spot)
                && a.f.standing()
                && a.f.charmed_by.is_none()
                && a.f.health < a.f.max_health * ORDERLY_BELOW
                && a.body.pos.distance(actors[i].body.pos) <= ORDERLY_REACH
        })
        .filter(|&k| in_sight(solids, me, actors[k].chest()))
        .min_by(|&x, &y| {
            let share = |a: &Actor| a.f.health / a.f.max_health.max(1.0);
            share(&actors[x])
                .total_cmp(&share(&actors[y]))
                .then(x.cmp(&y))
        });
    let Some(k) = hurt else {
        return false;
    };
    let mend = ORDERLY_MENDS * level_scale(actors[i].f.level);
    let before = actors[k].f.health;
    actors[k].f.health = (before + mend).min(actors[k].f.max_health);
    let mut e = event(
        tick,
        FxKind::Patch,
        actors[i].id,
        actors[k].id,
        actors[k].chest(),
    );
    e.amount = (actors[k].f.health - before).round() as u16;
    fx.push(e);
    true
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
        let (_, defeat, dealt) = hit(actors, i, k, dmg, false, tick);
        let mut e = event(tick, FxKind::Leap, me, actors[k].id, actors[k].chest());
        e.amount = dealt.round() as u16;
        e.defeat = defeat;
        fx.push(e);
    }
    fx.push(event(tick, FxKind::Leap, me, me, to));
}

/// The Doctor's Aether storm: when a hero comes within its ring he charges
/// it (a ring on the ground warns them), and it goes off round him a moment
/// later, hitting every hero still inside, in the air or not.
fn doctor_storm(
    actors: &mut [Actor],
    i: usize,
    en: &mut Enemy,
    tick: u64,
    foe_dist: f64,
    fx: &mut Vec<Fx>,
) {
    let me = actors[i].id;
    let at = actors[i].body.pos;
    match en.storm {
        Some(lands) if tick >= lands => {
            en.storm = None;
            let dmg = enemy_spec(EnemyKind::Doctor).damage
                * DOCTOR_STORM_HIT
                * level_scale(actors[i].f.level);
            for k in 0..actors.len() {
                let a = &actors[k];
                if a.hero.is_none()
                    || !a.f.standing()
                    || a.body.pos.distance(at) > DOCTOR_STORM_AREA
                {
                    continue;
                }
                let (_, defeat, dealt) = hit(actors, i, k, dmg, false, tick);
                let mut e = event(tick, FxKind::Pulse, me, actors[k].id, actors[k].chest());
                e.amount = dealt.round() as u16;
                e.defeat = defeat;
                fx.push(e);
            }
            fx.push(event(tick, FxKind::Pulse, me, me, at + DVec3::Y * 1.2));
        }
        None if tick >= en.next_trick && foe_dist < DOCTOR_STORM_AREA => {
            en.storm = Some(tick + ticks(DOCTOR_STORM_WINDUP));
            en.next_trick = tick + ticks(DOCTOR_STORM_WINDUP + DOCTOR_STORM_EVERY);
            // No blow of his own while he charges.
            en.windup = None;
            en.next_attack = en.next_attack.max(tick + ticks(DOCTOR_STORM_WINDUP + 0.5));
            fx.push(event(
                tick,
                FxKind::PulseWindup,
                me,
                me,
                at + DVec3::Y * 1.2,
            ));
        }
        _ => {}
    }
}

/// The hospital whose bay is nearest `at`.
fn nearest_ward(wards: &[hw_gen::urban::hospital::Ward], at: DVec3) -> Option<usize> {
    let flat = |p: DVec3| DVec2::new(p.x, p.z);
    (0..wards.len()).min_by(|&a, &b| {
        flat(wards[a].bay)
            .distance(flat(at))
            .total_cmp(&flat(wards[b].bay).distance(flat(at)))
    })
}

/// The first pod of hospital `ward` nobody floats in (the first one if
/// they are all taken).
fn free_pod(patients: &[Patient], ward: usize, pods: usize) -> usize {
    (0..pods)
        .find(|k| !patients.iter().any(|p| p.ward == ward && p.pod == *k))
        .unwrap_or(0)
}

/// How high the medical drone crosses town from `a` to `b`: well over the
/// roofs and landmarks near the way (those the heroes' square knows),
/// and over the hospital's own tower.
fn cruise_height(area: Option<&hw_gen::urban::area::Area>, a: DVec3, b: DVec3) -> f64 {
    let tower = b.y + hw_gen::urban::hospital::PYLON_TOP + 12.0;
    crate::taxi::express_height(area, a, b)
        .max(tower)
        .max(a.y + 15.0)
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
        // A hypnotised villain comes to its senses (or was beaten).
        if actors[i].f.charmed_by.is_some() && !actors[i].f.charmed(tick) {
            let a = &mut actors[i];
            let by = a.f.charmed_by.take().unwrap_or(a.id);
            a.f.charmed_until = 0;
            a.f.target = None;
            if a.f.fighting() {
                out.fx
                    .push(event(tick, FxKind::HypnoEnd, by, a.id, a.chest()));
            }
        }
        // A beaten sidekick lies there until `sidekick` takes it away.
        if actors[i].sidekick.is_some() {
            if actors[i].f.status == Status::Knocked && tick >= actors[i].f.until {
                actors[i].f.status = Status::Standing;
            }
            continue;
        }
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
                            && a.body.pos.distance(me)
                                <= REVIVE_REACH.max(a.f.kit.passive.revive_reach)
                    })
                    .map(|a| a.id);
                let fetching = fetched(actors[i].id, spawn_drones);
                let a = &mut actors[i];
                match helper {
                    Some(_) if !fetching => {
                        a.f.revive += DT;
                        if a.f.revive >= REVIVE_SECONDS {
                            a.f.status = Status::Standing;
                            a.f.health = a.f.max_health * REVIVE_HEALTH;
                            a.f.revive = 0.0;
                            a.f.last_combat = tick;
                            out.fx
                                .push(event(tick, FxKind::Revived, a.id, a.id, a.chest()));
                        }
                    }
                    _ if a.f.kit.passive.get_up > 0.0
                        && !fetching
                        && tick >= a.f.rise_ready
                        && tick >= a.f.down_at + ticks(a.f.kit.passive.get_up) =>
                    {
                        // A lifeline: the hero gets up on their own.
                        a.f.rise_ready = tick + ticks(GET_UP_EVERY);
                        a.f.status = Status::Standing;
                        a.f.health = a.f.max_health * REVIVE_HEALTH;
                        a.f.revive = 0.0;
                        a.f.last_combat = tick;
                        out.fx
                            .push(event(tick, FxKind::Revived, a.id, a.id, a.chest()));
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
            // A hero's health comes back in step with how much there is
            // (so level 20 heals as fast, for its size, as level 1);
            // regenerating powers add a share of it every second, and a
            // friend's aura brings energy back sooner.
            // Toggles that are on take their share of the energy, and a
            // hero in the air gets none back at all.
            let size = a.f.max_health / HERO_HEALTH;
            let vitality = a.f.kit.passive.regen * a.f.max_health;
            let rally = if a.f.rallied { RALLY_ENERGY } else { 1.0 };
            let upkeep = 1.0 - a.f.kit.upkeep();
            // (The town's own heroes are exempt: they fly to every fight.)
            let flying = !a.npc
                && a.hero.is_some_and(|(_, inp)| inp.fly)
                && a.f.kit.travels(powers::travel::FLY)
                && !a.body.grounded;
            (
                [HEALTH_REGEN.0, HEALTH_REGEN.1][k] * size + vitality,
                if flying {
                    0.0
                } else {
                    [ENERGY_REGEN.0, ENERGY_REGEN.1][k] * rally * upkeep
                },
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
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn a_minion_falls_in_one_or_two_hits() {
        for kind in [EnemyKind::Brute, EnemyKind::Technician] {
            let h = enemy_spec(kind).health;
            let bolt = def_of(PowerId(10)).fields.damage;
            let strike = def_of(PowerId::STRIKE).fields.damage;
            assert!(h <= bolt + strike, "{kind:?}");
            assert!(h > strike, "{kind:?} should take more than a punch");
        }
    }

    #[test]
    #[ignore = "on demand (looks): scripts/check.sh --on-demand"]
    fn bars_round_up_so_a_sliver_shows() {
        let mut f = Fighter::hero();
        f.health = 0.1;
        assert_eq!(f.view(0).health, 1);
        f.health = HERO_HEALTH;
        assert_eq!(f.view(0).health, 255);
    }
}
