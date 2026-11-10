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
use crate::progress::{is_passive, themes, Affinity};
use crate::robots::{self, Colossus, Mount};
use crate::{
    nav, step_hero, Control, Hero, Ids, LastChange, Motion, Net, Orientation, Progress, Prop,
    Solids, DT, GROUND_Y, TICK_HZ,
};
use bevy_ecs::prelude::*;
use glam::{DVec2, DVec3};
use hw_core::{NetId, Seed};
use hw_net::{
    fizzle, CombatState, EntityKind, Fx, FxKind, InputState, PlayerId, Power, Sheet, Status,
    EFFECT_ARMOURED, EFFECT_CAN_FLY, EFFECT_CARRYING, EFFECT_CHARMED, EFFECT_CHILLED,
    EFFECT_FROZEN, EFFECT_HELD, EFFECT_RALLIED, EFFECT_SHIELD, EFFECT_SUPER_JUMP,
    EFFECT_SUPER_SPEED, EFFECT_TOUGH,
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
        Power::SuperSpeed => PowerSpec {
            name: "Super Speed",
            ..none
        },
        // Damage is health given back.
        Power::Mend => PowerSpec {
            name: "Mend",
            damage: 30.0,
            cost: 18.0,
            recharge: 20.0,
            area: 14.0,
            ..none
        },
        // Area is the beam's half width.
        Power::Beam => PowerSpec {
            name: "Beam",
            damage: 50.0,
            cost: 22.0,
            recharge: 7.0,
            range: 36.0,
            knock: true,
            area: 1.4,
            ..none
        },
        Power::Vitality => PowerSpec {
            name: "Vitality",
            ..none
        },
        // Lasts is the delay before it lands.
        Power::Meteor => PowerSpec {
            name: "Meteor",
            damage: 32.0,
            cost: 30.0,
            recharge: 18.0,
            range: 45.0,
            area: 5.5,
            lasts: METEOR_DELAY,
            ..none
        },
        Power::Invulnerable => PowerSpec {
            name: "Invulnerable",
            ..none
        },
        Power::Nova => PowerSpec {
            name: "Nova",
            damage: 120.0,
            cost: 60.0,
            recharge: 40.0,
            knock: true,
            area: 16.0,
            ..none
        },
        Power::Rally => PowerSpec {
            name: "Rally",
            area: RALLY_RADIUS,
            ..none
        },
        // A cone toward the target: range its length, area the tangent of
        // its half angle, lasts how long a minion stays frozen.
        Power::Freeze => PowerSpec {
            name: "Freeze Ray",
            damage: 12.0,
            cost: 14.0,
            recharge: 16.0,
            range: 16.0,
            area: 0.42,
            lasts: FROZEN_SECONDS,
            ..none
        },
        Power::Teleport => PowerSpec {
            name: "Teleport",
            cost: 10.0,
            recharge: 3.0,
            range: TELEPORT_RANGE,
            ..none
        },
        Power::Bubble => PowerSpec {
            name: "Rescue Bubble",
            cost: 12.0,
            recharge: 8.0,
            range: BUBBLE_RANGE,
            ..none
        },
        // Damage is the sidekick's punch, recharge the wait before a
        // beaten one can be called again.
        Power::Sidekick => PowerSpec {
            name: "Robot Sidekick",
            damage: crate::sidekick::PUNCH_DAMAGE,
            cost: 30.0,
            recharge: 30.0,
            range: 40.0,
            ..none
        },
        // Lasts is how long a minion fights for the hero.
        Power::Hypno => PowerSpec {
            name: "Hypno Ray",
            cost: 20.0,
            recharge: 30.0,
            range: 30.0,
            lasts: CHARM_SECONDS,
            ..none
        },
    }
}

/// A minion frozen by a freeze ray stays in its ice this long (s); one hit
/// while frozen shatters it for this much more damage.
pub const FROZEN_SECONDS: f64 = 3.5;
pub const SHATTER: f32 = 2.0;
/// Tougher villains (and the rest of a cone) are chilled instead: slower
/// on their feet and slower to swing, for this long (s).
pub const CHILL_SECONDS: f64 = 6.0;
pub const CHILL_SPEED: f32 = 0.5;
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
/// Invulnerable lets this share of every blow through.
pub const TOUGH_LETS_THROUGH: f32 = 0.5;
/// Vitality gives back this share of the hero's health every second, in a
/// fight or not.
pub const VITALITY_REGEN: f32 = 0.02;

/// How many powers there are.
pub const POWERS: usize = Power::ALL.len();

fn slot(p: Power) -> usize {
    (p.code() - 1) as usize
}

/// The powers a hero owns and the theme of each, small enough to copy
/// about every tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Kit {
    owned: u32,
    themes: u32,
}

impl Kit {
    pub fn of(sheet: &Sheet) -> Self {
        let mut k = Kit::default();
        for p in &sheet.powers {
            k.owned |= 1 << p.power.code();
            k.themes |= ((p.theme & 1) as u32) << p.power.code();
        }
        k
    }

    /// Every power (villain-free tests, a cheat).
    pub fn everything() -> Self {
        Kit {
            owned: u32::MAX,
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
    pub ready: [u64; POWERS],
    /// A power waiting to fire, and the last tick it may still fire on.
    pub queued: Option<(Power, u64)>,
    /// Why the queued power has not fired yet (a [`hw_net::fizzle`] reason;
    /// 0 for none): told to the hero if it lapses.
    pub waiting: u16,
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
    /// Tick the held thing slips free if not thrown first.
    pub carry_until: u64,
    /// Effects a robot's state adds ([`EFFECT_ARMOURED`] and the like).
    pub marks: u16,
    /// Inside a Rally aura this tick (a hero's own counts).
    pub rallied: bool,
    /// Frozen solid in ice until this tick (a freeze ray).
    pub frozen_until: u64,
    /// Slowed by cold until this tick.
    pub chilled_until: u64,
    /// Fighting for the heroes until this tick (a hypno ray), and for whom.
    pub charmed_until: u64,
    pub charmed_by: Option<NetId>,
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
            ready: [0; POWERS],
            queued: None,
            waiting: 0,
            target: None,
            revive: 0.0,
            engaged: false,
            kit: Kit::default(),
            held_until: 0,
            shield_until: 0,
            carrying: None,
            carry_until: 0,
            marks: 0,
            rallied: false,
            frozen_until: 0,
            chilled_until: 0,
            charmed_until: 0,
            charmed_by: None,
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
            effects: (self.held(tick) as u16 * EFFECT_HELD)
                | (self.shielded(tick) as u16 * EFFECT_SHIELD)
                | (self.carrying.is_some() as u16 * EFFECT_CARRYING)
                | (self.kit.has(Power::Flight) as u16 * EFFECT_CAN_FLY)
                | (self.kit.has(Power::SuperJump) as u16 * EFFECT_SUPER_JUMP)
                | (self.kit.has(Power::SuperSpeed) as u16 * EFFECT_SUPER_SPEED)
                | (self.kit.has(Power::Invulnerable) as u16 * EFFECT_TOUGH)
                | (self.rallied as u16 * EFFECT_RALLIED)
                | (self.frozen(tick) as u16 * EFFECT_FROZEN)
                | ((self.chilled(tick) && !self.frozen(tick)) as u16 * EFFECT_CHILLED)
                | (self.charmed(tick) as u16 * EFFECT_CHARMED)
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
    /// The Doctor's Aether storm goes off at this tick.
    pub storm: Option<u64>,
    /// Its group's turf: it warns heroes off (see [`crate::factions`]).
    pub turf: bool,
    /// A hero this near provokes it (stepping into its crime, or a fight
    /// that is on).
    pub provoke: Option<f64>,
    /// The hero its group warned off, and when.
    pub warned: Option<(NetId, u64)>,
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
    pub theme: u8,
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
    pub theme: u8,
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
            routes: BTreeMap::new(),
            rescue_points: Vec::new(),
            rescues: Vec::new(),
            cold: Vec::new(),
            bombs: Vec::new(),
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
        let p = DVec2::new(spot.center.x, spot.center.z) + spot.facing * row + side * across;
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
    /// New meteors called down.
    sky: Vec<SkyStrike>,
    /// Blows at the city (where, radius, power).
    city: Vec<(DVec3, f32, f32)>,
    rewards: Vec<Reward>,
    deeds: Vec<Deed>,
    /// Sidekicks called: (owner, player, theme, where, level).
    summons: Vec<(NetId, PlayerId, u8, DVec3, u8)>,
    /// Rescue bubbles sent (see [`Fights::rescues`]).
    rescues: Vec<Rescue>,
    /// Freeze-ray cones (see [`Fights::cold`]).
    cold: Vec<(DVec3, DVec3, f64)>,
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
    heroes_act(&mut actors, &solids, &rescue_points, tick, &mut out);
    world.resource_mut::<Fights>().rescue_points = rescue_points;
    npc_heroes_act(&mut actors, &solids, tick, &mut out);
    sidekicks_act(&mut actors, &solids, tick, &mut out);
    land_throws(&mut actors, &mut throws, tick, &mut out);
    throws.append(&mut out.throws);
    land_sky(&mut actors, &mut sky, &solids, tick, &mut out);
    sky.append(&mut out.sky);
    enemies_act(
        &mut actors,
        &solids,
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
            let haste = if a.f.rallied { RALLY_HASTE } else { 1.0 };
            a.f.ready[slot(Power::Lift)] = tick + ticks(power_spec(Power::Lift).recharge / haste);
            let mut e = event(
                tick,
                FxKind::Throw,
                a.id,
                a.id,
                a.body.pos + DVec3::Y * CARRY_HEIGHT,
            );
            e.theme = a.f.kit.theme(Power::Lift);
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
    for (owner, player, theme, at, level) in std::mem::take(&mut out.summons) {
        crate::sidekick::spawn(world, owner, player, theme, at, level, tick);
    }
    robots::tidy(world, tick);
    let mut f = world.resource_mut::<Fights>();
    f.events = fx;
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
    strike(actors, from, to, damage, knock, tick, affinity, false)
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
    affinity: Option<Affinity>,
) -> (bool, bool, f32) {
    strike(actors, from, to, damage, knock, tick, affinity, true)
}

#[allow(clippy::too_many_arguments)]
fn strike(
    actors: &mut [Actor],
    from: usize,
    to: usize,
    damage: f32,
    knock: bool,
    tick: u64,
    affinity: Option<Affinity>,
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
    // A shield bearer holds its shield toward the one it fights.
    let shield_way = actors[to]
        .f
        .target
        .and_then(|f| actors.iter().find(|a| a.id == f))
        .map(|f| f.body.pos);
    let t = &mut actors[to];
    let mut dealt = if t.f.shielded(tick) {
        damage * (1.0 - SHIELD_BLOCKS)
    } else {
        damage
    };
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
    // A hero with Invulnerable: blows glance off.
    if t.hero.is_some() && t.f.kit.has(Power::Invulnerable) {
        dealt *= TOUGH_LETS_THROUGH;
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
    t.f.health = (t.f.health - dealt).max(floor);
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
        t.f.frozen_until = 0;
        t.f.charmed_until = 0;
        t.beaten_by = Some((credit, affinity));
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
            .filter(|&j| clear_line(solids, actors[i].chest(), actors[j].chest()))
        else {
            continue;
        };
        let damage = BOLT_DAMAGE * level_scale(actors[i].f.level);
        let (knocked, defeated, dealt) = hit_aimed(actors, i, j, damage, false, tick, None);
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
            || !clear_line(solids, actors[i].chest(), actors[j].chest())
        {
            continue;
        }
        let damage = power_spec(Power::Sidekick).damage * level_scale(actors[i].f.level);
        let affinity = themes(Power::Sidekick)[sk.theme as usize & 1].affinity;
        let (knock, defeat, dealt) = hit(actors, i, j, damage, false, tick, affinity);
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
        e.theme = sk.theme;
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
        .filter(|&(i, d)| d <= range && clear_line(solids, chest, actors[i].chest()))
        .min_by(|x, y| x.1.total_cmp(&y.1).then(x.0.cmp(&y.0)))
        .map(|(i, _)| i)
}

/// The affinity a hero's power leans toward, in the theme they chose.
fn power_affinity(kit: Kit, p: Power) -> Option<Affinity> {
    themes(p)[kit.theme(p) as usize].affinity
}

fn heroes_act(
    actors: &mut [Actor],
    solids: &Obstacles,
    rescue_points: &[DVec3],
    tick: u64,
    out: &mut Out,
) {
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
            Power::from_code(input.power).filter(|p| kit.has(*p) && !is_passive(*p))
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
            // A power still recharging waits for it, if not for long.
            let ready = actors[i].f.ready[slot(p)];
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
        if !actors[i].f.standing() {
            actors[i].f.queued = None;
            continue;
        }
        if tick > until {
            // It lapsed: say why, so a key press never just does nothing.
            actors[i].f.queued = None;
            let why = std::mem::take(&mut actors[i].f.waiting);
            if why != 0 {
                let me = actors[i].id;
                let mut e = event(tick, FxKind::Fizzle, me, me, actors[i].chest());
                e.amount = why | (kit.theme(power) as u16) << 8;
                e.theme = power.code();
                out.fx.push(e);
            }
            continue;
        }
        let spec = power_spec(power);
        let theme = kit.theme(power);
        let throwing = power == Power::Lift && actors[i].f.carrying.is_some();
        // A freeze ray goes where the hero aims when nothing is targeted
        // (at a fire, say), so it needs no target either.
        let free_aim = throwing || power == Power::Freeze;
        let aimed = matches!(
            power,
            Power::Strike
                | Power::Bolt
                | Power::Hold
                | Power::Beam
                | Power::Meteor
                | Power::Freeze
                | Power::Hypno
        ) || throwing;
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
                None if power == Power::Strike => {
                    nearest_seen_enemy(actors, solids, actors[i].chest(), spec.range)
                }
                None => {
                    let chest = actors[i].chest();
                    let reach = spec.range.min(25.0);
                    actors
                        .iter()
                        .enumerate()
                        .filter(|(_, a)| hittable(a))
                        // Only what the hero can see: never a villain
                        // behind a wall when one in the open is nearer
                        // the aim.
                        .filter(|(_, a)| clear_line(solids, chest, a.chest()))
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
                let in_reach = actors[t].body.pos.distance(pos) <= spec.range;
                // No blow and no power through a wall, a punch included.
                let seen = clear_line(solids, actors[i].chest(), actors[t].chest());
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
                actors[i].f.waiting = why;
                continue;
            }
            // Bosses and machines shrug off hypnosis.
            if power == Power::Hypno
                && target.is_some_and(|t| actors[t].enemy.is_none_or(|e| !charmable(e.kind)))
            {
                actors[i].f.queued = None;
                continue;
            }
            // From here the blow is the villain's, whether it lands now or
            // once the power is ready.
            actors[i].f.engaged = target.is_some();
        }
        if matches!(power, Power::Shockwave | Power::Nova)
            && nearest_enemy(actors, pos, spec.area).is_some()
        {
            actors[i].f.engaged = true;
        }
        // A sidekick already out is sent at the target: free and at once.
        let my_sidekick = actors
            .iter()
            .position(|a| a.sidekick.is_some_and(|s| s.owner == actors[i].id) && a.f.fighting());
        let commanding = power == Power::Sidekick && my_sidekick.is_some();
        // Where a teleport lands, and which rescue a bubble goes to: none,
        // and the power waits (and then lapses) without costing anything.
        let blink = if power == Power::Teleport {
            match teleport_to(solids, actors[i].body.pos, aim) {
                Some(p) => Some(p),
                None => {
                    actors[i].f.waiting = fizzle::NO_SPOT;
                    continue;
                }
            }
        } else {
            None
        };
        let rescue = if power == Power::Bubble {
            match nearest_rescue(rescue_points, actors[i].chest(), aim) {
                Some(p) => Some(p),
                None => {
                    actors[i].f.waiting = fizzle::NO_SPOT;
                    continue;
                }
            }
        } else {
            None
        };
        let me = &actors[i].f;
        // Lifting paid for the throw; a lift waits for the last throw's recharge.
        let cost = if throwing || commanding {
            0.0
        } else {
            spec.cost
        };
        let ready = throwing || commanding || tick >= me.ready[slot(power)];
        if tick < me.busy_until || !ready || me.energy < cost {
            actors[i].f.waiting = if !ready {
                fizzle::RECHARGING
            } else if me.energy < cost {
                fizzle::ENERGY
            } else {
                fizzle::BUSY
            };
            continue;
        }
        let me_id = actors[i].id;
        let me = &mut actors[i].f;
        me.queued = None;
        me.energy -= cost;
        me.busy_until = tick + ticks(ACTION_LOCK);
        if (power != Power::Lift || throwing) && !commanding {
            let haste = if me.rallied { RALLY_HASTE } else { 1.0 };
            me.ready[slot(power)] = tick + ticks(spec.recharge / haste);
        }
        let rallied = if me.rallied && power != Power::Mend {
            RALLY_DAMAGE
        } else {
            1.0
        };
        let damage = spec.damage * level_scale(me.level) * rallied;
        let level = me.level;
        let affinity = power_affinity(kit, power);
        match power {
            Power::Strike | Power::Bolt | Power::Hold => {
                let Some(t) = target else {
                    continue;
                };
                actors[i].f.target = Some(actors[t].id);
                // A bolt or a punch can meet a shield; a hold's field
                // closes round it.
                let blow = if power == Power::Hold { hit } else { hit_aimed };
                let (knock, defeat, dealt) = blow(actors, i, t, damage, spec.knock, tick, affinity);
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
            Power::Shockwave | Power::Nova => {
                let kind = if power == Power::Nova {
                    FxKind::Nova
                } else {
                    FxKind::Shockwave
                };
                let chest = actors[i].chest();
                let caught = burst(
                    actors,
                    i,
                    Burst {
                        centre: pos,
                        sight: Some(chest),
                        area: spec.area,
                        damage,
                        kind,
                        theme,
                        affinity,
                        knock: spec.knock,
                    },
                    solids,
                    tick,
                    out,
                );
                actors[i].f.last_combat = tick;
                let mut e = event(tick, kind, me_id, me_id, pos);
                e.amount = caught;
                e.theme = theme;
                out.fx.push(e);
                if power == Power::Nova {
                    // The street round the hero takes it too.
                    let blast = crate::destruction::hero_power(level).blast;
                    out.city.push((pos + DVec3::Y * 0.5, 4.0, blast * 2.0));
                }
            }
            Power::Mend => {
                for (k, a) in actors.iter_mut().enumerate() {
                    if a.hero.is_none() || !a.f.fighting() || a.body.pos.distance(pos) > spec.area {
                        continue;
                    }
                    let before = a.f.health;
                    a.f.health = (a.f.health + damage).min(a.f.max_health);
                    let mut e = event(tick, FxKind::Mend, me_id, a.id, a.chest());
                    e.amount = (a.f.health - before).round() as u16;
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
            Power::Beam => {
                let Some(t) = target else {
                    continue;
                };
                actors[i].f.target = Some(actors[t].id);
                let from = actors[i].chest();
                let dir = (actors[t].chest() - from).normalize_or(DVec3::X);
                // The beam runs on to its full length or the first wall.
                let mut len = spec.range;
                let steps = (spec.range / 0.5) as usize;
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
                        (along > 0.0 && along <= len && off <= spec.area).then_some((k, along))
                    })
                    .collect();
                struck.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
                for (k, _) in struck {
                    let (knock, defeat, dealt) = hit(actors, i, k, damage, true, tick, affinity);
                    let mut e = event(tick, FxKind::Beam, me_id, actors[k].id, actors[k].chest());
                    e.amount = dealt.round() as u16;
                    e.knock = knock;
                    e.defeat = defeat;
                    e.theme = theme;
                    out.fx.push(e);
                }
                actors[i].f.last_combat = tick;
                let mut e = event(tick, FxKind::Beam, me_id, me_id, from + dir * len);
                e.theme = theme;
                out.fx.push(e);
            }
            Power::Meteor => {
                let Some(t) = target else {
                    continue;
                };
                actors[i].f.target = Some(actors[t].id);
                let at = actors[t].body.pos;
                out.sky.push(SkyStrike {
                    land: tick + ticks(spec.lasts),
                    from: me_id,
                    at,
                    damage,
                    theme,
                    level,
                });
                actors[i].f.last_combat = tick;
                let mut e = event(tick, FxKind::MeteorCall, me_id, actors[t].id, at);
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
            Power::Freeze => {
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
                let mut len = spec.range;
                for s in 1..=(spec.range / 0.5) as usize {
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
                        let wide = 0.8 + along * spec.area;
                        (along > 0.0 && along <= len + 0.6 && off <= wide).then_some((k, along))
                    })
                    .filter(|&(k, _)| clear_line(solids, from, actors[k].chest()))
                    .collect();
                caught.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
                for (k, _) in caught {
                    let (_, defeat, dealt) = hit(actors, i, k, damage, false, tick, affinity);
                    // Minions freeze solid; tougher ones (and the big
                    // machines) only slow down.
                    let freezes = actors[k].enemy.is_some_and(|e| freezable(e.kind));
                    if !defeat {
                        let f = &mut actors[k].f;
                        if freezes {
                            f.frozen_until = tick + ticks(spec.lasts);
                            f.held_until = 0;
                            if let Some(en) = actors[k].enemy.as_mut() {
                                en.windup = None;
                            }
                        }
                        actors[k].f.chilled_until = tick + ticks(CHILL_SECONDS);
                    }
                    let mut e = event(tick, FxKind::Freeze, me_id, actors[k].id, actors[k].chest());
                    e.amount = dealt.round() as u16;
                    e.knock = freezes && !defeat;
                    e.defeat = defeat;
                    e.theme = theme;
                    out.fx.push(e);
                }
                actors[i].f.last_combat = tick;
                out.cold.push((from, dir, len));
                let mut e = event(tick, FxKind::Freeze, me_id, me_id, from + dir * len);
                e.theme = theme;
                out.fx.push(e);
            }
            Power::Teleport => {
                let Some(to) = blink else {
                    continue;
                };
                let from = actors[i].body.pos;
                let mut e = event(tick, FxKind::TeleportOut, me_id, me_id, from);
                e.theme = theme;
                out.fx.push(e);
                let b = &mut actors[i].body;
                b.pos = to;
                // Arrives at rest (falls if it is in the air).
                b.vel = DVec3::ZERO;
                b.grounded = false;
                let mut e = event(tick, FxKind::TeleportIn, me_id, me_id, to);
                e.theme = theme;
                out.fx.push(e);
            }
            Power::Bubble => {
                let Some(at) = rescue else {
                    continue;
                };
                out.rescues.push(Rescue {
                    hero: me_id,
                    level,
                    at,
                });
            }
            Power::Sidekick => {
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
                match my_sidekick {
                    Some(k) => {
                        if let Some(f) = foe {
                            actors[k].f.target = Some(f);
                        }
                        let mut e = event(tick, FxKind::SidekickCall, me_id, actors[k].id, pos);
                        e.theme = theme;
                        out.fx.push(e);
                    }
                    None => {
                        let Some((player, _)) = actors[i].hero else {
                            continue;
                        };
                        // At the hero's side, a step back.
                        let back = DVec3::new(-aim.x, 0.0, -aim.z).normalize_or(DVec3::Z);
                        let side = DVec3::new(back.z, 0.0, -back.x);
                        let mut at = pos + back * 1.4 + side * 1.2;
                        if solids.solid_at(at + DVec3::Y * 0.6) {
                            at = pos;
                        }
                        out.summons.push((me_id, player, theme, at, level));
                        let mut e = event(tick, FxKind::SidekickCall, me_id, me_id, at);
                        e.theme = theme;
                        out.fx.push(e);
                    }
                }
            }
            Power::Hypno => {
                let Some(t) = target else {
                    continue;
                };
                let lieutenant = actors[t]
                    .enemy
                    .is_some_and(|e| enemy_spec(e.kind).rank != "Minion");
                let secs = spec.lasts * if lieutenant { CHARM_LIEUTENANT } else { 1.0 };
                let v = &mut actors[t];
                v.f.charmed_until = tick + ticks(secs);
                v.f.charmed_by = Some(me_id);
                v.f.target = None;
                v.f.held_until = 0;
                if let Some(en) = v.enemy.as_mut() {
                    en.windup = None;
                }
                let charmed = actors[t].id;
                // Nobody keeps after the hero's new friend's old target.
                if actors[i].f.target == Some(charmed) {
                    actors[i].f.target = None;
                }
                actors[i].f.last_combat = tick;
                let mut e = event(tick, FxKind::Hypno, me_id, charmed, actors[t].chest());
                e.amount = secs.round() as u16;
                e.theme = theme;
                out.fx.push(e);
            }
            Power::SuperJump
            | Power::Flight
            | Power::SuperSpeed
            | Power::Vitality
            | Power::Invulnerable
            | Power::Rally => {}
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
    if aim == DVec3::ZERO {
        return None;
    }
    let eye = from + DVec3::Y * (BODY_HEIGHT * 0.8);
    let step = 0.5;
    let mut reach = TELEPORT_RANGE;
    for s in 1..=(TELEPORT_RANGE / step) as usize {
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
    while d >= 3.0 {
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

/// The trapped or hurt person a rescue bubble goes to: within reach, the
/// one nearest where the hero aims.
fn nearest_rescue(points: &[DVec3], chest: DVec3, aim: DVec3) -> Option<DVec3> {
    points
        .iter()
        .copied()
        .filter(|p| p.distance(chest) <= BUBBLE_RANGE)
        .min_by(|a, b| {
            let off = |p: DVec3| {
                let to = p - chest;
                let along = to.dot(aim).max(0.0);
                (to - aim * along).length() + to.length() * 0.05
            };
            off(*a).total_cmp(&off(*b))
        })
}

/// A blast all round a point (shockwave, nova, a meteor landing).
struct Burst {
    centre: DVec3,
    /// Where walls are checked from (a blast does not go through them);
    /// None for a blast from the sky, which nothing shelters from.
    sight: Option<DVec3>,
    area: f64,
    damage: f32,
    kind: FxKind,
    theme: u8,
    affinity: Option<Affinity>,
    /// Knocks everyone it hits down (not only those it defeats).
    knock: bool,
}

/// Hits every villain in `b`'s area that the blast can reach, knocking
/// them down; returns how many.
fn burst(
    actors: &mut [Actor],
    i: usize,
    b: Burst,
    solids: &Obstacles,
    tick: u64,
    out: &mut Out,
) -> u16 {
    let me_id = actors[i].id;
    let mut caught = 0;
    for k in 0..actors.len() {
        if !hittable(&actors[k])
            || actors[k].body.pos.distance(b.centre) > b.area
            || b.sight
                .is_some_and(|s| !clear_line(solids, s, actors[k].chest()))
        {
            continue;
        }
        let (knock, defeat, dealt) = hit(actors, i, k, b.damage, b.knock, tick, b.affinity);
        let mut e = event(tick, b.kind, me_id, actors[k].id, actors[k].chest());
        e.amount = dealt.round() as u16;
        e.knock = knock;
        e.defeat = defeat;
        e.theme = b.theme;
        out.fx.push(e);
        caught += 1;
    }
    caught
}

/// Who stands in a Rally aura this tick: every hero within reach of a
/// standing hero who owns Rally (their own aura too).
fn rally(actors: &mut [Actor]) {
    let auras: Vec<DVec3> = actors
        .iter()
        .filter(|a| a.hero.is_some() && a.f.fighting() && a.f.kit.has(Power::Rally))
        .map(|a| a.body.pos)
        .collect();
    for a in actors.iter_mut() {
        a.f.rallied = a.hero.is_some()
            && a.f.fighting()
            && auras.iter().any(|p| p.distance(a.body.pos) <= RALLY_RADIUS);
    }
}

/// Meteors that land this tick: a blast where they come down, and a
/// crater in the street.
fn land_sky(
    actors: &mut [Actor],
    sky: &mut Vec<SkyStrike>,
    solids: &Obstacles,
    tick: u64,
    out: &mut Out,
) {
    let due: Vec<SkyStrike> = sky.iter().copied().filter(|m| m.land <= tick).collect();
    sky.retain(|m| m.land > tick);
    let spec = power_spec(Power::Meteor);
    for m in due {
        let Some(i) = actors.iter().position(|a| a.id == m.from) else {
            continue;
        };
        let affinity = themes(Power::Meteor)[m.theme as usize & 1].affinity;
        burst(
            actors,
            i,
            Burst {
                centre: m.at,
                sight: None,
                area: spec.area,
                damage: m.damage,
                kind: FxKind::Meteor,
                theme: m.theme,
                affinity,
                knock: spec.knock,
            },
            solids,
            tick,
            out,
        );
        actors[i].f.last_combat = tick;
        let mut e = event(tick, FxKind::Meteor, m.from, m.from, m.at);
        e.theme = m.theme;
        out.fx.push(e);
        let blast = crate::destruction::hero_power(m.level).blast;
        out.city.push((m.at + DVec3::Y * 0.5, 3.5, blast * 3.0));
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
        // Beaten by one of the town's heroes: nobody's karma.
        if actors.iter().any(|a| a.id == by && a.npc) {
            continue;
        }
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
    id: NetId,
    tick: u64,
    here: DVec3,
    goal: DVec3,
) -> DVec2 {
    let flat = |a: DVec3, b: DVec3| DVec2::new(b.x - a.x, b.z - a.z);
    let direct = flat(here, goal).normalize_or_zero();
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
        if actors[i].f.chilled(tick) {
            spec.windup *= CHILL_WINDUP;
            spec.speed *= CHILL_SPEED as f64;
        }
        if actors[i].f.standing() && !getting_up && !held {
            // Who to fight: the foe while it stands and is not led too far
            // from home, else the nearest standing hero it notices. A
            // hypnotised villain fights its own kind instead, and its own
            // kind fights it.
            let leash = 45.0;
            let foe_ok = |a: &Actor| {
                if charmed {
                    hittable(a)
                } else {
                    (a.hero.is_some() || a.npc || a.sidekick.is_some() || a.f.charmed(tick))
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
                    .filter(|&(j, d)| d < CHARM_REACH && clear_line(solids, me, actors[j].chest()))
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
                        *d < spec.aggro && (en.perch || clear_line(solids, me, actors[*j].chest()))
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
                                    && clear_line(solids, actors[i].chest(), actors[t].chest());
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
                            go = heading(paths, solids, id, tick, here, there) * spec.speed;
                        } else if dist < near {
                            go = -dir * spec.speed * 0.8;
                        }
                        let can_attack = dist <= spec.reach && seen && tick >= en.next_attack;
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
            let (knock, defeat, dealt) = hit(actors, i, k, b.damage, false, tick, None);
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
        .filter(|&k| clear_line(solids, me, actors[k].chest()))
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
        let (_, defeat, dealt) = hit(actors, i, k, dmg, false, tick, None);
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
                let (_, defeat, dealt) = hit(actors, i, k, dmg, false, tick, None);
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
            // A hero's health comes back in step with how much there is
            // (so level 20 heals as fast, for its size, as level 1);
            // Vitality adds a share of it every second, and a Rally aura
            // brings energy back sooner.
            let size = a.f.max_health / HERO_HEALTH;
            let vitality = if a.f.kit.has(Power::Vitality) {
                VITALITY_REGEN * a.f.max_health
            } else {
                0.0
            };
            let rally = if a.f.rallied { RALLY_ENERGY } else { 1.0 };
            (
                [HEALTH_REGEN.0, HEALTH_REGEN.1][k] * size + vitality,
                [ENERGY_REGEN.0, ENERGY_REGEN.1][k] * rally,
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
