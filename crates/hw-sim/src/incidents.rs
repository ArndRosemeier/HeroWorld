//! Incidents: trouble that comes to the heroes. Every so often something
//! starts within a few hundred metres of a hero: a mugging, a robbery at a
//! shop, a mutant rampage, a breakout from one of the mad scientist's labs,
//! a car crash with people trapped, a fire with people inside.
//!
//! A fight is a group of villains standing where it happens; beating them
//! all resolves it (karma for everyone near). A rescue is people to get
//! out: a hero holds the help key (the revive key) next to each in turn
//! until they are free (Guardian karma for each). The last one out is hurt:
//! the hero who freed them carries them to the ambulance waiting down the
//! street, and only then is the rescue done. A carrier knocked down drops
//! them; anyone can pick them up again with the same key. One nobody
//! answers ends after a few minutes (the crooks get away, or the fire
//! brigade comes).
//!
//! The spawn rule (DESIGN.md): two to four open at once within about 800 m
//! of the heroes, at least a minute apart. Which and where come from the
//! world's seed and a running count, so a host restarted from the same
//! save sees the same sequence.

use crate::combat::{self, EnemyKind, Fighter, Fights, Spot};
use crate::progress;
use crate::{Control, Hero, Motion, Net, Solids, DT, GROUND_Y, TICK_HZ};
use bevy_ecs::prelude::*;
use glam::{DVec2, DVec3};
use hw_core::{det, NetId, Seed};
use hw_gen::building::BuildingUse;
use hw_gen::urban::area::Area;
use hw_gen::urban::landmarks::LandmarkKind;
use hw_net::FxKind;
use hw_net::PlayerId;
use hw_net::ToClient;
use std::collections::BTreeMap;
use std::sync::Arc;

/// At least this long between two incidents starting (s).
pub const GAP_SECONDS: f64 = 60.0;
/// The first one starts this long after the world does (s).
pub const FIRST_SECONDS: f64 = 20.0;
/// An incident nobody answers ends after this long (s).
pub const TIMEOUT_SECONDS: f64 = 240.0;
/// A hero this near has answered it (m).
pub const ANSWER: f64 = 60.0;
/// Heroes this near share the karma when it is resolved (m).
pub const REWARD: f64 = 120.0;
/// Incidents start this far from a hero (m) who flies or runs at super
/// speed: the whole town is near for them.
pub const NEAREST: f64 = 150.0;
pub const FARTHEST: f64 = 800.0;
/// For a hero on foot, this far (m): a minute's run at most.
pub const NEAREST_AFOOT: f64 = 50.0;
pub const FARTHEST_AFOOT: f64 = 350.0;
/// The streets this near a gang's hangout (m) are rough: crime comes
/// about twice as often there, and more of it is the gang's muggings.
pub const HOT_REACH: f64 = 450.0;
pub const HOT_GAP: f64 = 30.0;
/// A hero closing in on an incident from this near (m) is on the way: it
/// waits for them (it does not time out, and the town's own heroes leave it
/// to them) ...
pub const COMING_REACH: f64 = 1200.0;
/// ... as long as they keep closing in (they may stop for this long, s) ...
const COMING_PAUSE: f64 = 12.0;
/// ... up to this long past its usual time (s).
const COMING_MOST: f64 = 300.0;
/// A colossus nobody answers goes home after this long (s).
pub const COLOSSUS_TIMEOUT: f64 = 600.0;
/// The colossus comes out at most this often (s), and not before the team
/// reaches this level.
pub const COLOSSUS_GAP: f64 = 1500.0;
pub const COLOSSUS_LEVEL: u8 = 3;
/// A hero this near a trapped person can help them out (m).
pub const RESCUE_REACH: f64 = 3.5;
/// The ambulance parks this far from junctions (m).
const MEDIC_CLEAR: f64 = 15.0;
/// Carrying the hurt this near the medic hands them over (m).
pub const MEDIC_REACH: f64 = 5.0;
/// The state an open fire's news carries once a freeze ray put it out.
pub const DOUSED: u8 = 3;
/// A doused fire: this much longer before it ends on its own (s), and the
/// people come out this much quicker.
pub const DOUSED_GRACE: f64 = 120.0;
pub const DOUSED_QUICKER: f64 = 0.6;
/// A freeze ray puts out a fire whose front it passes this near (m).
pub const DOUSE_REACH: f64 = 4.5;
/// Seconds of holding the help key to lift someone lying hurt.
pub const LIFT_SECONDS: f64 = 1.0;
/// The ambulance waits about this far from a rescue (m).
const MEDIC_AWAY: f64 = 50.0;
/// A hero holding the help key this near a frightened mole talks the
/// group down (m), in this many seconds.
pub const CALM_REACH: f64 = 4.5;
pub const CALM_SECONDS: f64 = 3.0;
/// Talked down, the mole folk climb back into their hole: gone after this
/// long (s).
pub const HOME_SECONDS: f64 = 6.0;
/// A fight's call stands only while its villains do: once none of them
/// (standing, down or carried) is this near it (m) for
/// [`STRAY_SECONDS`], they got away and the call ends. With none left at
/// all it ends at once. A call never marks an empty street.
pub const STRAY_REACH: f64 = 80.0;
pub const STRAY_SECONDS: f64 = 30.0;
/// The Iron-Masked Empire lands only once the team reaches this level.
pub const IRON_LEVEL: u8 = 2;
/// The invaders from Zorrak come down only once the team reaches this
/// level.
pub const SAUCER_LEVEL: u8 = 2;

/// News state: unanswered, its crooks are making off with what they took.
pub const GETTING_AWAY: u8 = 4;
/// How far they run before they are gone (m).
const FLEE_FAR: f64 = 70.0;
/// Seconds they have to get away before they are gone for good.
const FLEE_MOST: f64 = 50.0;
/// One this near where it is running to is gone (m).
const FLEE_HOME: f64 = 2.5;
/// Nor while a hero is this near: it runs on instead (m).
const FLEE_SEEN: f64 = 45.0;
/// A hero this near one making off has caught up with them (m).
pub const CATCH_UP: f64 = 22.0;
/// A rampage's villains smash something in the street every this many
/// seconds (one of them, if nothing else is going on)...
const SMASH_EVERY: f64 = 7.0;
/// ...within this many metres of the incident.
const SMASH_REACH: f64 = 18.0;

/// What kind of trouble.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IncidentKind {
    /// Two thugs robbing someone in the street.
    Mugging,
    /// A gang holding up a shop.
    Robbery,
    /// Mutant brutes smashing up a street.
    Rampage,
    /// The mad scientist's technicians loose from a lab.
    Breakout,
    /// Two cars smashed together, people trapped inside.
    CarCrash,
    /// A house on fire, people inside.
    Fire,
    /// The mad scientist's walking tank marches out of his quarter.
    Colossus,
    /// A building brought down in a fight (by heroes or the colossus),
    /// people trapped in the rubble. Never rolled: it follows a collapse.
    Collapse,
    /// Mole folk burst up through the street, frightened by the Lab's
    /// drilling. Talk them down (hold the help key by them) and they go
    /// home, and Molehaven remembers; or fight them.
    Surfacing,
    /// Sky pirates rope down from their airship to loot a shop.
    PirateRaid,
    /// An iron capsule of the Iron-Masked Empire crashes into a street;
    /// its legionaries round up townsfolk to take away.
    IronLanding,
    /// A flying saucer from Zorrak hangs over a street and beams townsfolk
    /// up, its bug-eyed troopers keeping everyone back.
    SaucerRaid,
}

impl IncidentKind {
    pub const ALL: [IncidentKind; 12] = [
        IncidentKind::Mugging,
        IncidentKind::Robbery,
        IncidentKind::Rampage,
        IncidentKind::Breakout,
        IncidentKind::CarCrash,
        IncidentKind::Fire,
        IncidentKind::Colossus,
        IncidentKind::Collapse,
        IncidentKind::Surfacing,
        IncidentKind::PirateRaid,
        IncidentKind::IronLanding,
        IncidentKind::SaucerRaid,
    ];

    pub fn code(self) -> u8 {
        self as u8
    }

    pub fn from_code(c: u8) -> Option<Self> {
        Self::ALL.get(c as usize).copied()
    }

    /// What the wrist radio calls it.
    pub fn name(self) -> &'static str {
        match self {
            IncidentKind::Mugging => "Mugging",
            IncidentKind::Robbery => "Robbery in progress",
            IncidentKind::Rampage => "Mutant rampage",
            IncidentKind::Breakout => "Lab breakout",
            IncidentKind::CarCrash => "Car crash, someone trapped",
            IncidentKind::Fire => "Fire, people inside",
            IncidentKind::Colossus => "The colossus is loose!",
            IncidentKind::Collapse => "Building collapse, people trapped",
            IncidentKind::Surfacing => "Mole folk surfacing",
            IncidentKind::PirateRaid => "Sky pirates raiding a shop",
            IncidentKind::IronLanding => "Iron Legion landing, people taken",
            IncidentKind::SaucerRaid => "Flying saucer beaming people up",
        }
    }

    /// People to get out (none for a fight).
    pub fn trapped(self) -> u8 {
        match self {
            // One: the hero carries them out, and only one at a time.
            IncidentKind::CarCrash => 1,
            IncidentKind::Fire | IncidentKind::Collapse => 3,
            _ => 0,
        }
    }

    /// Seconds of holding the help key to get one person out.
    pub fn rescue_seconds(self) -> f64 {
        match self {
            IncidentKind::CarCrash => 4.0,
            // Digging them out.
            IncidentKind::Collapse => 5.0,
            _ => 3.0,
        }
    }

    /// The group: how many, which kind only, levels above the team.
    fn group(self) -> (u32, Option<EnemyKind>, u8) {
        match self {
            IncidentKind::Mugging => (2, Some(EnemyKind::Greaser), 0),
            IncidentKind::Robbery => (4, Some(EnemyKind::Gunsel), 1),
            IncidentKind::Rampage => (3, Some(EnemyKind::Brute), 2),
            IncidentKind::Breakout => (5, Some(EnemyKind::Technician), 2),
            IncidentKind::Colossus => (1, Some(EnemyKind::Colossus), 3),
            IncidentKind::Surfacing => (4, Some(EnemyKind::Delver), 0),
            IncidentKind::PirateRaid => (5, None, 1),
            IncidentKind::IronLanding => (5, None, 2),
            IncidentKind::SaucerRaid => (5, None, 2),
            IncidentKind::CarCrash | IncidentKind::Fire | IncidentKind::Collapse => (0, None, 0),
        }
    }

    /// Its faction's crew, when it is not of one kind only: (who fights up
    /// close, who shoots).
    fn crew(self) -> Option<(EnemyKind, EnemyKind)> {
        match self {
            IncidentKind::PirateRaid => Some((EnemyKind::Buccaneer, EnemyKind::Gunner)),
            IncidentKind::IronLanding => Some((EnemyKind::Legionary, EnemyKind::Legionary)),
            IncidentKind::SaucerRaid => Some((EnemyKind::Grabber, EnemyKind::Zapper)),
            _ => None,
        }
    }

    /// People the villains hold (townsfolk rounded up), freed when it ends.
    pub fn captives(self) -> u8 {
        match self {
            IncidentKind::IronLanding | IncidentKind::SaucerRaid => 2,
            _ => 0,
        }
    }

    /// Karma for resolving it, on top of the villains' own.
    fn bonus(self) -> f32 {
        match self {
            IncidentKind::Mugging => 1.0,
            IncidentKind::Robbery => 1.5,
            IncidentKind::Rampage => 2.0,
            IncidentKind::Breakout => 2.5,
            // Per person got out (a crash has one: as much as two used
            // to pay).
            IncidentKind::CarCrash => 6.0,
            IncidentKind::Fire => 2.5,
            IncidentKind::Colossus => 1.5,
            IncidentKind::Collapse => 3.0,
            // Fought rather than talked down: a small thanks from the
            // street, none from Molehaven.
            IncidentKind::Surfacing => 0.5,
            IncidentKind::PirateRaid => 2.0,
            IncidentKind::IronLanding => 3.0,
            IncidentKind::SaucerRaid => 3.0,
        }
    }

    /// A hero this near has stepped in, and the crooks turn on them (m).
    /// Further off they get on with the crime, unless they would fight the
    /// hero anyway (see [`crate::factions`]).
    pub fn reach(self) -> f64 {
        match self {
            IncidentKind::Mugging => 9.0,
            IncidentKind::Robbery => 12.0,
            IncidentKind::Rampage => 16.0,
            IncidentKind::Breakout => 18.0,
            // The walking tank fights whoever comes.
            IncidentKind::Colossus => f64::INFINITY,
            // Frightened, not wicked: they fight only who hits them.
            IncidentKind::Surfacing => 0.0,
            IncidentKind::PirateRaid => 12.0,
            // Hostile to everyone anyway; this is how near they defend
            // their captives even from a friend.
            IncidentKind::IronLanding => 20.0,
            // The same: they guard the people in the beam.
            IncidentKind::SaucerRaid => 20.0,
            IncidentKind::CarCrash | IncidentKind::Fire | IncidentKind::Collapse => 0.0,
        }
    }

    /// The lieutenant or boss in front of its group.
    fn lead(self) -> Option<EnemyKind> {
        match self {
            IncidentKind::Rampage => Some(EnemyKind::Alpha),
            IncidentKind::Breakout => Some(EnemyKind::Sentry),
            IncidentKind::PirateRaid => Some(EnemyKind::Captain),
            IncidentKind::IronLanding => Some(EnemyKind::Praetorian),
            IncidentKind::SaucerRaid => Some(EnemyKind::Overlord),
            _ => None,
        }
    }

    /// Unanswered, its villains make off in plain view (rather than just
    /// being gone): every street crime and raid. The walking tank and the
    /// mole folk go home as before.
    pub fn makes_off(self) -> bool {
        matches!(
            self,
            IncidentKind::Mugging
                | IncidentKind::Robbery
                | IncidentKind::Rampage
                | IncidentKind::Breakout
                | IncidentKind::PirateRaid
                | IncidentKind::IronLanding
        )
    }

    /// They make off to their own ride, standing where the call is (the
    /// pirates' ladders, the legion's capsule), so the call stays put.
    pub fn flees_to_its_ride(self) -> bool {
        matches!(self, IncidentKind::PirateRaid | IncidentKind::IronLanding)
    }

    /// What its villains carry off, the first `n` of them (see
    /// [`hw_net::protocol::loot`]).
    fn loot(self) -> (u16, usize) {
        use hw_net::protocol::loot;
        match self {
            IncidentKind::Mugging => (loot::HANDBAG, 1),
            IncidentKind::Robbery => (loot::SACK, 2),
            IncidentKind::Breakout => (loot::CRATE, 1),
            IncidentKind::PirateRaid => (loot::CRATE, 2),
            _ => (loot::NONE, 0),
        }
    }

    /// Seconds it waits for heroes before it ends unanswered.
    fn timeout(self) -> f64 {
        match self {
            IncidentKind::Colossus => COLOSSUS_TIMEOUT,
            _ => TIMEOUT_SECONDS,
        }
    }
}

/// How an incident stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Open,
    Resolved,
    TimedOut,
}

impl State {
    fn code(self) -> u8 {
        match self {
            State::Open => 0,
            State::Resolved => 1,
            State::TimedOut => 2,
        }
    }
}

/// Where a rescue's hurt person is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hurt {
    /// On the ground, waiting to be lifted.
    Lying(DVec3),
    /// In a hero's arms.
    Carried(NetId),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Incident {
    pub id: u32,
    pub kind: IncidentKind,
    pub at: DVec3,
    /// Its villains' fight spot (none for a rescue).
    pub spot: Option<usize>,
    /// Which way it faces (a crash: across the road; a fire: out of the
    /// house).
    pub facing: DVec2,
    /// People still to get out.
    pub left: u8,
    /// Seconds a hero has been helping the next one out (or lifting the
    /// hurt one).
    pub freeing: f64,
    /// A rescue's hurt person, once out.
    pub hurt: Option<Hurt>,
    /// Where a rescue's ambulance waits, and which way it faces.
    pub medic: Option<(DVec3, DVec2)>,
    pub level: u8,
    pub started: u64,
    /// Unanswered, it ends at this tick.
    pub expires: u64,
    pub answered: bool,
    pub state: State,
    /// Mole folk talked down at this tick: they are on their way home.
    pub calmed: Option<u64>,
    /// The nearest hero's distance at the last second (m).
    pub nearest: f64,
    /// A hero is on the way until this tick (see [`COMING_REACH`]).
    pub coming_until: u64,
    /// A fire put out by a freeze ray: it burns no longer (the people
    /// inside hold out much longer, and come out sooner).
    pub doused: bool,
    /// Each player's collateral tally when it started: what they break
    /// before it is won decides the careful-hero bonus.
    pub collateral_from: BTreeMap<PlayerId, f64>,
    /// The help bar last sent (see [`Incident::held`]).
    pub held_sent: u8,
    /// A fight's villains have all been away from it since this tick
    /// (see [`STRAY_REACH`]).
    pub strayed: Option<u64>,
    /// Unanswered, its crooks started making off at this tick (see
    /// [`GETTING_AWAY`]).
    pub fleeing: Option<u64>,
    /// How many of them got clean away.
    pub escaped: u32,
    /// A hero caught up with them as they made off: beating the rest wins.
    pub caught: bool,
    /// The next smash in a rampage (tick).
    pub next_smash: u64,
}

impl Incident {
    /// Is a hero on their way to it (closing in from not too far)?
    pub fn awaited(&self, tick: u64) -> bool {
        !self.answered && tick < self.coming_until
    }

    /// Notes how near the nearest hero is now (`d`, m), once a second: one
    /// closing in keeps it waiting for them.
    fn watch(&mut self, tick: u64, d: f64) {
        let closing = d < self.nearest - 2.0 && d < COMING_REACH;
        self.nearest = d;
        if !closing || self.answered {
            return;
        }
        self.coming_until = tick + ticks(COMING_PAUSE);
        // Time for them to get here at a run, and to spare.
        let eta = d / crate::RUN_SPEED * 1.5 + 30.0;
        let most = self.started + ticks(self.kind.timeout() + COMING_MOST);
        self.expires = self.expires.max((tick + ticks(eta)).min(most));
    }

    fn news(&self) -> ToClient {
        ToClient::Incident {
            id: self.id,
            kind: self.kind.code(),
            at: self.at.as_vec3().to_array(),
            level: self.level,
            state: match self.state {
                State::Open if self.fleeing.is_some() => GETTING_AWAY,
                State::Open if self.doused => DOUSED,
                s => s.code(),
            },
            yaw: crate::props::yaw_to_byte(self.facing.x.atan2(self.facing.y)),
            left: self.left,
            held: self.held(),
        }
    }

    /// How far along the help key is held here, in sixteenths: getting the
    /// next one out, lifting the hurt one, or talking mole folk down.
    pub fn held(&self) -> u8 {
        let need = if self.kind == IncidentKind::Surfacing {
            CALM_SECONDS
        } else if matches!(self.hurt, Some(Hurt::Lying(_))) {
            LIFT_SECONDS
        } else if self.doused {
            self.kind.rescue_seconds() * DOUSED_QUICKER
        } else {
            self.kind.rescue_seconds()
        };
        ((self.freeing / need).clamp(0.0, 1.0) * 16.0).floor() as u8
    }

    /// The hurt person's news (none for a fight or before anyone is out).
    fn casualty(&self, delivered: bool) -> Option<ToClient> {
        let (medic, face) = self.medic?;
        let (state, carrier, at) = match (delivered, self.hurt) {
            (true, _) => (2, 0, medic),
            (false, Some(Hurt::Lying(p))) => (0, 0, p),
            (false, Some(Hurt::Carried(h))) => (1, h.0, self.at),
            (false, None) => return None,
        };
        Some(ToClient::Casualty {
            id: self.id,
            medic: medic.as_vec3().to_array(),
            yaw: crate::props::yaw_to_byte(face.x.atan2(face.y)),
            state,
            carrier,
            at: at.as_vec3().to_array(),
        })
    }

    /// Where the people still trapped are (the next to get out last).
    pub fn victims(&self) -> Vec<DVec3> {
        victims(self.kind, self.at, self.facing, self.left)
    }
}

/// Where the trapped people of an incident are: a crash's at the door
/// of its first car (a second would be at the other's), a fire's at the door and windows of the house.
/// The server and every client place them alike.
pub fn victims(kind: IncidentKind, at: DVec3, facing: DVec2, left: u8) -> Vec<DVec3> {
    let f = DVec3::new(facing.x, 0.0, facing.y).normalize_or(DVec3::X);
    let side = DVec3::new(f.z, 0.0, -f.x);
    (0..left)
        .map(|k| match kind {
            IncidentKind::CarCrash => {
                let car = if k % 2 == 0 { -1.0 } else { 1.0 };
                at + side * (2.6 * car) + f * 1.5
            }
            _ => {
                let n = kind.trapped().max(1) as f64;
                at - f * 2.2 + side * ((k as f64 - (n - 1.0) * 0.5) * 1.8)
            }
        })
        .collect()
}

/// A townsperson caught up in a crime (the one being mugged): they cower
/// while it lasts, then walk off. Not a fighter: heroes cannot target
/// townsfolk.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Bystander {
    pub incident: u32,
    /// Walking off: the way, and the tick they are gone.
    pub leaving: Option<(DVec2, u64)>,
    /// Standing to thank their hero until this tick, before walking off.
    pub thanking_until: u64,
    /// Which of the citizens' looks they have, when it must be a
    /// particular one (someone a hero saw before the crime was stepped
    /// in on: see [`crate::crimes`]); else their id picks.
    pub look: Option<u8>,
    /// Marched off to the Iron-Masked capsule (their legionaries got away
    /// with them): gone once there.
    pub march: Option<DVec3>,
}

/// How fast someone set free indoors hurries out (m/s).
pub const ESCAPE_SPEED: f64 = 2.2;

/// The way out for someone set free indoors (a case's hostage): the
/// points still to walk to, in order; gone after the last.
#[derive(Component, Clone, Debug, Default)]
pub struct WayOut(pub Vec<DVec3>);

/// Seconds someone saved stands thanking their hero before they go.
pub const THANK_SECONDS: f64 = 3.0;
/// A hero this near when it is over gets thanked (m).
pub const THANK_REACH: f64 = 30.0;

/// Seconds a bystander walks off before they are gone.
pub const LEAVE_SECONDS: f64 = 12.0;
/// Their walking pace (m/s).
pub const LEAVE_SPEED: f64 = 1.4;
/// How fast captives are marched off (m/s).
const MARCH_SPEED: f64 = 1.8;
/// How far ahead (m) someone walking off looks for something in the way.
const LEAVE_LOOK: f64 = 0.5;

/// Where the one being mugged stands: in front of the muggers (who stand
/// 1.5 m ahead of the spot, facing this way).
pub fn mugged(at: DVec3, facing: DVec2) -> DVec3 {
    at + DVec3::new(facing.x, 0.0, facing.y) * 3.4
}

/// Where the townsfolk an incident's villains hold stand: between the
/// legionaries (behind the spot) and their praetorian (in front), facing
/// the way the group does.
pub fn captives(kind: IncidentKind, at: DVec3, facing: DVec2) -> Vec<DVec3> {
    let f = DVec3::new(facing.x, 0.0, facing.y).normalize_or(DVec3::X);
    let side = DVec3::new(f.z, 0.0, -f.x);
    let n = kind.captives();
    (0..n)
        .map(|k| at - f * 1.0 + side * ((k as f64 - (n as f64 - 1.0) * 0.5) * 1.6))
        .collect()
}

fn yaw_quat(dir: DVec2) -> [f32; 4] {
    glam::Quat::from_rotation_y(dir.x.atan2(dir.y) as f32).to_array()
}

pub(crate) fn spawn_bystander(
    world: &mut World,
    tick: u64,
    incident: u32,
    at: DVec3,
    facing: DVec2,
    look: Option<u8>,
) -> NetId {
    // On the ground where they stand, not at the spot's height: a few
    // metres off it on a slope, that is half a metre up or down.
    let at = match world.get_resource::<Solids>() {
        Some(s) if s.0.inside(at).is_none() => {
            let y = s.0.ground_for(DVec2::new(at.x, at.z), at.y).max(GROUND_Y);
            if (y - at.y).abs() < 1.5 {
                DVec3::new(at.x, y, at.z)
            } else {
                at
            }
        }
        _ => at,
    };
    let id = world.resource_mut::<crate::Ids>().0.allocate();
    world.spawn((
        Net(id),
        Motion(crate::collision::Body {
            pos: at,
            vel: DVec3::ZERO,
            grounded: true,
        }),
        crate::Orientation(yaw_quat(facing)),
        Bystander {
            incident,
            leaving: None,
            thanking_until: 0,
            look,
            march: None,
        },
        crate::LastChange(tick),
    ));
    id
}

/// The crime is over: its bystanders turn to the nearest hero and thank
/// them, then walk off, away from where it was.
pub(crate) fn release_bystanders(world: &mut World, tick: u64, incident: u32) {
    let mut heroes: Vec<(NetId, DVec3)> = world
        .query::<(&Hero, &Net, &Motion, &Fighter)>()
        .iter(world)
        .filter(|(_, _, _, f)| f.standing())
        .map(|(_, n, m, _)| (n.0, m.0.pos))
        .collect();
    heroes.sort_by_key(|h| h.0);
    let mut fx = Vec::new();
    for (n, m, mut b, mut o) in world
        .query::<(&Net, &Motion, &mut Bystander, &mut crate::Orientation)>()
        .iter_mut(world)
    {
        if b.incident != incident || b.leaving.is_some() {
            continue;
        }
        // Turn aside from the muggers and go.
        let q = glam::Quat::from_array(o.0);
        let f = q * glam::Vec3::Z;
        let away = DVec2::new(f.z as f64, -f.x as f64).normalize_or(DVec2::X);
        let hero = heroes
            .iter()
            .filter(|h| h.1.distance(m.0.pos) < THANK_REACH)
            .min_by(|a, b| a.1.distance(m.0.pos).total_cmp(&b.1.distance(m.0.pos)));
        let mut wait = 0.0;
        if let Some(&(h, at)) = hero {
            // First a word of thanks, facing them.
            let to = DVec2::new(at.x - m.0.pos.x, at.z - m.0.pos.z);
            o.0 = yaw_quat(to.normalize_or(DVec2::Y));
            b.thanking_until = tick + ticks(THANK_SECONDS);
            wait = THANK_SECONDS;
            let mut e = combat::event(tick, hw_net::FxKind::Thanked, n.0, h, m.0.pos);
            e.amount = (n.0 .0 % 64) as u16;
            fx.push(e);
        }
        b.leaving = Some((away, tick + ticks(LEAVE_SECONDS + wait)));
    }
    world.resource_mut::<Fights>().events.extend(fx);
}

/// People set free indoors walking out along their [`WayOut`] (after
/// thanking their hero), and gone at its end.
pub(crate) fn walk_ways(world: &mut World, tick: u64) {
    let mut gone = Vec::new();
    for (e, b, mut m, mut o, mut way) in world
        .query::<(
            Entity,
            &Bystander,
            &mut Motion,
            &mut crate::Orientation,
            &mut WayOut,
        )>()
        .iter_mut(world)
    {
        if b.leaving.is_none() {
            continue;
        }
        if tick < b.thanking_until {
            m.0.vel = DVec3::ZERO;
            continue;
        }
        let mut left = ESCAPE_SPEED * DT;
        let mut pos = m.0.pos;
        let mut dir = DVec3::ZERO;
        while left > 0.0 {
            let Some(&to) = way.0.first() else { break };
            let d = to - pos;
            let len = d.length();
            if len <= left {
                pos = to;
                left -= len;
                way.0.remove(0);
            } else {
                dir = d / len;
                pos += dir * left;
                left = 0.0;
            }
        }
        if way.0.is_empty() {
            gone.push(e);
            continue;
        }
        m.0.pos = pos;
        m.0.vel = dir * ESCAPE_SPEED;
        let flat = DVec2::new(dir.x, dir.z);
        if flat.length() > 1e-3 {
            let q = yaw_quat(flat.normalize());
            if o.0 != q {
                o.0 = q;
            }
        }
    }
    for e in gone {
        world.despawn(e);
    }
}

/// Bystanders walking off, and gone once out of sight.
fn walk_bystanders(world: &mut World, tick: u64) {
    let solids = world.resource::<Solids>().0.clone();
    let mut gone = Vec::new();
    for (e, mut b, mut m, mut o) in world
        .query_filtered::<(Entity, &mut Bystander, &mut Motion, &mut crate::Orientation), Without<WayOut>>()
        .iter_mut(world)
    {
        if let (Some(to), None) = (b.march, b.leaving) {
            // Marched off to the capsule, and in: gone.
            let flat = DVec2::new(to.x - m.0.pos.x, to.z - m.0.pos.z);
            if flat.length() < 1.2 {
                gone.push(e);
                continue;
            }
            let d = flat.normalize();
            let step = d * MARCH_SPEED * DT;
            let next = DVec2::new(m.0.pos.x + step.x, m.0.pos.z + step.y);
            let y = solids.ground_for(next, m.0.pos.y).max(GROUND_Y);
            m.0.pos = DVec3::new(next.x, y, next.y);
            m.0.vel = DVec3::new(d.x, 0.0, d.y) * MARCH_SPEED;
            let q = yaw_quat(d);
            if o.0 != q {
                o.0 = q;
            }
            continue;
        }
        let Some((dir, until)) = b.leaving else {
            continue;
        };
        if tick >= until {
            gone.push(e);
            continue;
        }
        if tick < b.thanking_until {
            // Still thanking their hero.
            m.0.vel = DVec3::ZERO;
            continue;
        }
        let mut body = m.0;
        // Straight on, or turned aside as little as will do where a wall,
        // a bench or a step is in the way; the new way is kept.
        let free = |d: DVec2| {
            let step = d * LEAVE_SPEED * DT;
            let next = DVec2::new(body.pos.x + step.x, body.pos.z + step.y);
            let y = solids.ground_for(next, body.pos.y).max(GROUND_Y);
            let ahead =
                DVec3::new(body.pos.x, y, body.pos.z) + DVec3::new(d.x, 0.0, d.y) * LEAVE_LOOK;
            ((y - body.pos.y).abs() < 0.6
                && solids.passable_at(ahead + DVec3::Y * 0.3)
                && solids.passable_at(ahead + DVec3::Y * 1.2))
            .then_some((next, y))
        };
        let turned = [0.0, 45.0, -45.0, 90.0, -90.0, 135.0, -135.0, 180.0]
            .into_iter()
            .map(|a: f64| {
                let (s, c) = (det::sin(a.to_radians()), det::cos(a.to_radians()));
                DVec2::new(dir.x * c - dir.y * s, dir.x * s + dir.y * c)
            })
            .find_map(|d| free(d).map(|n| (d, n)));
        let dir = match turned {
            Some((d, (next, y))) => {
                body.pos = DVec3::new(next.x, y, next.y);
                body.vel = DVec3::new(d.x, 0.0, d.y) * LEAVE_SPEED;
                if d != dir {
                    b.leaving = Some((d, until));
                }
                d
            }
            None => {
                body.vel = DVec3::ZERO;
                dir
            }
        };
        m.0 = body;
        let q = yaw_quat(dir);
        if o.0 != q {
            o.0 = q;
        }
    }
    for e in gone {
        world.despawn(e);
    }
}

/// The town's incidents.
#[derive(Default)]
pub struct Incidents {
    town: Option<Arc<Area>>,
    /// Open ones.
    pub open: Vec<Incident>,
    /// How many have started (seeds the next).
    pub count: u32,
    last_start: Option<u64>,
    /// Seconds before the first one (default [`FIRST_SECONDS`]).
    pub first_after: f64,
    /// The kind the next one will be, if set (tests and screenshots).
    pub next_kind: Option<IncidentKind>,
    /// Screenshots: the next rescue starts with everyone out and the hurt
    /// one in the arms of the hero it started near, who stands beside it.
    pub carried_start: bool,
    /// Screenshots: the next rescue starts with the hero nearest it
    /// standing by the next trapped person, ready to help them out.
    pub beside_start: bool,
    /// Start one of this kind at the next second, however many are open
    /// and whenever the last one started (the debug menu).
    pub forced: Option<IncidentKind>,
    /// Seconds a new one waits for heroes, in place of its kind's own
    /// (screenshots of crooks making off: `HW_TIMEOUT`).
    pub timeout: Option<f64>,
    /// Spots of incidents that ended, free to use again.
    free: Vec<usize>,
    /// When the colossus last came out.
    last_colossus: Option<u64>,
    /// Gang hangouts: the streets round them are rough (see [`HOT_REACH`]).
    hot: Vec<DVec2>,
    /// Buildings that came down since the last second (people trapped).
    collapses: Vec<u64>,
    news: Vec<ToClient>,
    /// Every player's collateral tally now (the server copies it in
    /// before each step; see [`crate::destruction::Damage::collateral`]).
    pub(crate) collateral: BTreeMap<PlayerId, f64>,
}

/// Two to four open at once: more with more heroes.
pub fn wanted(heroes: usize) -> usize {
    (1 + heroes).clamp(2, 4)
}

/// A hero as incidents see them: who, where, their level, whether they
/// are on foot (neither flight nor super speed), and their player.
type Seen = (NetId, DVec3, u8, bool, PlayerId);

/// A fight won while breaking at most this much of the street (credits:
/// a window or two, a lamp post) earns the careful-hero bonus.
pub const CAREFUL_COST: f64 = 2_000.0;
/// The bonus: this share of the fight's karma on top.
pub const CAREFUL_BONUS: f32 = 0.15;

fn ticks(seconds: f64) -> u64 {
    (seconds * TICK_HZ as f64).round() as u64
}

impl Incidents {
    pub fn new(town: Arc<Area>) -> Self {
        Self {
            town: Some(town),
            first_after: FIRST_SECONDS,
            ..Default::default()
        }
    }

    /// The gangs' hangouts: crime is more common round them.
    pub fn set_hot(&mut self, hangouts: Vec<DVec2>) {
        self.hot = hangouts;
    }

    /// Is `p` in the rough streets round a gang's hangout?
    pub fn is_hot(&self, p: DVec3) -> bool {
        let p = DVec2::new(p.x, p.z);
        self.hot.iter().any(|h| h.distance(p) < HOT_REACH)
    }

    /// New incidents come in another square of the town (open ones stay
    /// where they are).
    pub fn set_town(&mut self, town: Arc<Area>) {
        self.town = Some(town);
    }

    /// Every tick: helping people out. Once a second: start, answer,
    /// resolve and time out incidents.
    pub(crate) fn step(&mut self, world: &mut World, tick: u64) {
        if self.town.is_none() {
            return;
        }
        self.douse(world, tick);
        self.bubbles(world, tick);
        self.rescue(world);
        self.publish_rescue_points(world);
        self.calm(world, tick);
        // The help bar: news each time it moves on a sixteenth (or back).
        for inc in &mut self.open {
            let held = inc.held();
            if held != inc.held_sent {
                inc.held_sent = held;
                self.news.push(inc.news());
            }
        }
        walk_bystanders(world, tick);
        if !tick.is_multiple_of(TICK_HZ as u64) {
            return;
        }
        let mut heroes: Vec<Seen> = world
            .query::<(&Hero, &Net, &Motion, &Fighter)>()
            .iter(world)
            .map(|(h, n, m, f)| {
                let afoot = !f.kit.travels(crate::powers::travel::FLY)
                    && !f.kit.travels(crate::powers::travel::SPEED);
                (n.0, m.0.pos, f.level, afoot, h.owner)
            })
            .collect();
        heroes.sort_by_key(|h| h.0);
        self.got_away(world, &heroes);
        self.settle(world, tick, &heroes);
        for b in std::mem::take(&mut self.collapses) {
            self.start_collapse(world, tick, b);
        }
        if let Some(kind) = self.forced.take() {
            // A few tries: a place can be taken by another incident.
            let open = self.open.len();
            for _ in 0..8 {
                if heroes.is_empty() || self.open.len() > open {
                    break;
                }
                self.next_kind = Some(kind);
                self.start(world, tick, &heroes);
            }
            self.next_kind = None;
            return;
        }
        // More, and more often, in the rough streets round a gang.
        let hot = heroes.iter().any(|h| self.is_hot(h.1));
        let most = wanted(heroes.len()) + hot as usize;
        if heroes.is_empty() || self.open.len() >= most {
            return;
        }
        let gap = if hot { HOT_GAP } else { GAP_SECONDS };
        let due = match self.last_start {
            None => tick >= ticks(self.first_after),
            Some(t) => tick >= t + ticks(gap),
        };
        if due {
            self.start(world, tick, &heroes);
        }
    }

    /// Building `b` (a collider of the city's solids) started to come
    /// down: people are trapped in it.
    pub fn collapsed(&mut self, b: u64) {
        if self.town.is_some() && !self.collapses.contains(&b) {
            self.collapses.push(b);
        }
    }

    /// A rescue in the rubble of building `b`, at its front.
    fn start_collapse(&mut self, world: &mut World, tick: u64, b: u64) {
        let solids = world.resource::<Solids>().0.clone();
        let Some((plan, _)) = solids.building(b) else {
            return;
        };
        let s = &plan.site;
        let p = s.origin + s.along * s.size.x * 0.5 - s.inward * 3.0;
        // One rescue per pile of rubble.
        if self
            .open
            .iter()
            .any(|i| DVec2::new(i.at.x, i.at.z).distance(p) < 30.0)
        {
            return;
        }
        self.count += 1;
        let y = solids.ground_at(p).max(GROUND_Y);
        let inc = self.rescue_at(
            world,
            tick,
            IncidentKind::Collapse,
            DVec3::new(p.x, y, p.y),
            (-s.inward).normalize_or(DVec2::X),
        );
        self.news.push(inc.news());
        self.news.extend(inc.casualty(false));
        self.open.push(inc);
    }

    /// A new rescue of `kind` at `at`, facing out of its place; the
    /// ambulance waits on a straight street near.
    fn rescue_at(
        &self,
        world: &mut World,
        tick: u64,
        kind: IncidentKind,
        at: DVec3,
        facing: DVec2,
    ) -> Incident {
        let town = self.town.as_ref().expect("a town");
        let level = world.resource::<Fights>().team_level;
        let medic = medic_spot(town, &world.resource::<Solids>().0, DVec2::new(at.x, at.z));
        Incident {
            id: self.count,
            kind,
            at,
            spot: None,
            facing,
            left: kind.trapped(),
            freeing: 0.0,
            hurt: None,
            medic: Some(medic),
            level,
            started: tick,
            expires: tick + ticks(self.timeout.unwrap_or(kind.timeout())),
            answered: false,
            state: State::Open,
            calmed: None,
            nearest: f64::INFINITY,
            coming_until: 0,
            doused: false,
            collateral_from: self.collateral.clone(),
            held_sent: 0,
            strayed: None,
            fleeing: None,
            escaped: 0,
            caught: false,
            next_smash: 0,
        }
    }

    /// Heroes holding the help key next to the next trapped person get
    /// them out, one after another; the last one out is carried to the
    /// medic.
    /// Freeze-ray cones this tick put out the fires they pass.
    fn douse(&mut self, world: &mut World, tick: u64) {
        let cold = world.resource::<Fights>().cold.clone();
        if cold.is_empty() {
            return;
        }
        for inc in self
            .open
            .iter_mut()
            .filter(|i| i.kind == IncidentKind::Fire && !i.doused && i.state == State::Open)
        {
            let f = DVec3::new(inc.facing.x, 0.0, inc.facing.y).normalize_or(DVec3::X);
            let front = inc.at - f * 3.0 + DVec3::Y * 2.0;
            let hit = cold.iter().any(|&(from, dir, len)| {
                let along = (front - from).dot(dir).clamp(0.0, len);
                (from + dir * along).distance(front) < DOUSE_REACH
            });
            if hit {
                inc.doused = true;
                inc.answered = true;
                inc.expires = inc.expires.max(tick) + ticks(DOUSED_GRACE);
                self.news.push(inc.news());
            }
        }
    }

    /// Rescue bubbles sent this tick: each takes the trapped person it was
    /// sent to out to the street (karma as if freed by hand), or the hurt
    /// one straight to the ambulance (the rescue done).
    fn bubbles(&mut self, world: &mut World, tick: u64) {
        let sent = world.resource::<Fights>().rescues.clone();
        if sent.is_empty() {
            return;
        }
        let near: Vec<(NetId, DVec3, u8)> = world
            .query::<(&Hero, &Net, &Motion, &Fighter)>()
            .iter(world)
            .map(|(_, n, m, f)| (n.0, m.0.pos, f.level))
            .collect();
        let mut rewards = Vec::new();
        let mut fx = Vec::new();
        for r in sent {
            let Some(inc) = self.open.iter_mut().find(|i| {
                i.medic.is_some()
                    && (i.victims().iter().any(|v| v.distance(r.at) < 0.5)
                        || matches!(i.hurt, Some(Hurt::Lying(p)) if p.distance(r.at) < 0.5))
            }) else {
                continue;
            };
            let (medic, _) = inc.medic.expect("a rescue");
            let base = combat::enemy_spec(EnemyKind::Brute).karma;
            let mut pay = |x: f32, at: DVec3| {
                for h in near
                    .iter()
                    .filter(|h| h.1.distance(at) < REWARD || h.0 == r.hero)
                {
                    let one = progress::defeat_karma(base, inc.level, h.2) as f32;
                    rewards.push(combat::Reward {
                        hero: h.0,
                        karma: (one * inc.kind.bonus() * x).round() as u32,
                    });
                }
            };
            inc.answered = true;
            let lying = matches!(inc.hurt, Some(Hurt::Lying(p)) if p.distance(r.at) < 0.5);
            let last = inc.left == 1 && !lying;
            let mut e = combat::event(tick, FxKind::Bubble, r.hero, r.hero, r.at);
            e.amount = (inc.id & 0xffff) as u16;
            e.defeat = lying || last;
            fx.push(e);
            if lying || last {
                // The hurt one floats straight to the medic.
                if last {
                    inc.left = 0;
                    inc.freeing = 0.0;
                    pay(1.0, r.at);
                    self.news.push(inc.news());
                }
                inc.hurt = None;
                pay(2.0, medic);
                let news = inc.casualty(true);
                self.news.extend(news);
                fx.push(combat::event(
                    tick,
                    FxKind::BubbleLand,
                    r.hero,
                    r.hero,
                    medic,
                ));
            } else {
                // Out to the street in front of the place.
                inc.left -= 1;
                inc.freeing = 0.0;
                pay(1.0, r.at);
                self.news.push(inc.news());
                let f = DVec3::new(inc.facing.x, 0.0, inc.facing.y).normalize_or(DVec3::X);
                let out = DVec3::new(r.at.x, inc.at.y, r.at.z) + f * 7.0;
                fx.push(combat::event(tick, FxKind::BubbleLand, r.hero, r.hero, out));
            }
        }
        let mut fights = world.resource_mut::<Fights>();
        fights.rewards.extend(rewards);
        fights.events.extend(fx);
    }

    /// Where people wait to be rescued, for rescue bubbles to find.
    fn publish_rescue_points(&self, world: &mut World) {
        let mut points = Vec::new();
        for inc in self.open.iter().filter(|i| i.medic.is_some()) {
            if let Some(Hurt::Lying(p)) = inc.hurt {
                points.push(p);
            }
            if inc.hurt.is_none() {
                points.extend(inc.victims());
            }
        }
        world.resource_mut::<Fights>().rescue_points = points;
    }

    fn rescue(&mut self, world: &mut World) {
        if self.open.iter().all(|i| i.medic.is_none()) {
            return;
        }
        let mut helpers: Vec<(NetId, DVec3, u8, bool, bool)> = world
            .query::<(&Hero, &Net, &Motion, &Fighter, &Control)>()
            .iter(world)
            .map(|(_, n, m, f, c)| {
                let up = f.standing();
                (n.0, m.0.pos, f.level, c.0.revive && up, up)
            })
            .collect();
        helpers.sort_by_key(|h| h.0);
        let mut rewards = Vec::new();
        let guardian = |rewards: &mut Vec<combat::Reward>, inc: &Incident, near: DVec3, x: f32| {
            let base = combat::enemy_spec(EnemyKind::Brute).karma;
            for h in helpers.iter().filter(|h| h.1.distance(near) < REWARD) {
                let one = progress::defeat_karma(base, inc.level, h.2) as f32;
                rewards.push(combat::Reward {
                    hero: h.0,
                    karma: (one * inc.kind.bonus() * x).round() as u32,
                });
            }
        };
        for inc in self.open.iter_mut().filter(|i| i.medic.is_some()) {
            let (medic, _) = inc.medic.expect("a rescue");
            match inc.hurt {
                Some(Hurt::Carried(id)) => {
                    let carrier = helpers.iter().find(|h| h.0 == id);
                    match carrier {
                        Some(h) if h.4 => {
                            let flat = DVec2::new(h.1.x - medic.x, h.1.z - medic.z).length();
                            if flat < MEDIC_REACH && (h.1.y - medic.y).abs() < 4.0 {
                                // Handed over: the rescue is done.
                                let news = inc.casualty(true);
                                inc.hurt = None;
                                guardian(&mut rewards, inc, medic, 2.0);
                                self.news.extend(news);
                            }
                        }
                        // Knocked down (or gone): they drop them.
                        _ => {
                            let at = carrier.map_or(inc.at, |h| h.1);
                            inc.hurt = Some(Hurt::Lying(at));
                            inc.freeing = 0.0;
                            self.news.extend(inc.casualty(false));
                        }
                    }
                    continue;
                }
                Some(Hurt::Lying(at)) => {
                    let lifter = helpers
                        .iter()
                        .find(|h| h.3 && h.1.distance(at) < RESCUE_REACH);
                    match lifter {
                        None => inc.freeing = (inc.freeing - DT).max(0.0),
                        Some(h) => {
                            inc.freeing += DT;
                            if inc.freeing + 1e-9 >= LIFT_SECONDS {
                                inc.freeing = 0.0;
                                inc.hurt = Some(Hurt::Carried(h.0));
                                self.news.extend(inc.casualty(false));
                            }
                        }
                    }
                    continue;
                }
                None if inc.left == 0 => continue,
                None => {}
            }
            let next = *inc.victims().last().expect("someone trapped");
            let helper = helpers
                .iter()
                .filter(|h| h.3 && h.1.distance(next) < RESCUE_REACH)
                .min_by(|a, b| a.1.distance(next).total_cmp(&b.1.distance(next)));
            let Some(helper) = helper else {
                inc.freeing = (inc.freeing - DT).max(0.0);
                continue;
            };
            inc.answered = true;
            inc.freeing += DT;
            let quicker = if inc.doused { DOUSED_QUICKER } else { 1.0 };
            if inc.freeing + 1e-9 < inc.kind.rescue_seconds() * quicker {
                continue;
            }
            inc.freeing = 0.0;
            inc.left -= 1;
            // Karma for everyone who helped or stood by.
            guardian(&mut rewards, inc, next, 1.0);
            self.news.push(inc.news());
            if inc.left == 0 {
                // The last one out is hurt: in the arms of who freed them.
                inc.hurt = Some(Hurt::Carried(helper.0));
                self.news.extend(inc.casualty(false));
            }
        }
        world.resource_mut::<Fights>().rewards.extend(rewards);
    }

    /// Heroes holding the help key next to frightened mole folk talk them
    /// down: they thank the hero, Molehaven likes the heroes there better,
    /// and the group climbs back into its hole. Not once a fight is on.
    fn calm(&mut self, world: &mut World, tick: u64) {
        if !self
            .open
            .iter()
            .any(|i| i.kind == IncidentKind::Surfacing && i.calmed.is_none())
        {
            return;
        }
        let mut heroes: Vec<(NetId, DVec3, u8, bool)> = world
            .query::<(&Hero, &Net, &Motion, &Fighter, &Control)>()
            .iter(world)
            .map(|(_, n, m, f, c)| (n.0, m.0.pos, f.level, c.0.revive && f.standing()))
            .collect();
        heroes.sort_by_key(|h| h.0);
        let mut moles: Vec<(NetId, usize, DVec3, bool)> = world
            .query::<(&Net, &combat::Enemy, &Motion, &Fighter)>()
            .iter(world)
            .map(|(n, e, m, f)| {
                let calm = f.standing() && f.target.is_none();
                (n.0, e.spot, m.0.pos, calm)
            })
            .collect();
        moles.sort_by_key(|m| m.0);
        let mut rewards = Vec::new();
        let mut friends = Vec::new();
        let mut fx = Vec::new();
        let mut homeward: Vec<(usize, DVec3)> = Vec::new();
        for inc in self
            .open
            .iter_mut()
            .filter(|i| i.kind == IncidentKind::Surfacing && i.calmed.is_none())
        {
            let Some(spot) = inc.spot else {
                continue;
            };
            let group: Vec<&(NetId, usize, DVec3, bool)> =
                moles.iter().filter(|m| m.1 == spot).collect();
            if group.is_empty() || group.iter().any(|m| !m.3) {
                // A fight is on (or over): no talking now.
                inc.freeing = 0.0;
                continue;
            }
            let talker = heroes
                .iter()
                .find(|h| h.3 && group.iter().any(|m| m.2.distance(h.1) < CALM_REACH));
            let Some(talker) = talker else {
                inc.freeing = (inc.freeing - DT).max(0.0);
                continue;
            };
            inc.answered = true;
            inc.freeing += DT;
            if inc.freeing + 1e-9 < CALM_SECONDS {
                continue;
            }
            inc.freeing = 0.0;
            inc.calmed = Some(tick);
            inc.left = 0;
            self.news.push(inc.news());
            let base = combat::enemy_spec(EnemyKind::Delver).karma;
            for h in heroes.iter().filter(|h| h.1.distance(inc.at) < REWARD) {
                let one = progress::defeat_karma(base, inc.level, h.2) as f32;
                rewards.push(combat::Reward {
                    hero: h.0,
                    karma: (one * group.len() as f32).round() as u32,
                });
                friends.push((h.0, hw_net::Faction::Moles, crate::factions::CALMED));
            }
            let lead = group[0];
            fx.push(combat::event(
                tick,
                hw_net::FxKind::Calmed,
                lead.0,
                talker.0,
                lead.2 + DVec3::Y * 1.6,
            ));
            homeward.push((spot, inc.at));
        }
        for (mut en, mut f) in world
            .query::<(&mut combat::Enemy, &mut Fighter)>()
            .iter_mut(world)
        {
            if let Some((_, at)) = homeward.iter().find(|h| h.0 == en.spot) {
                en.home = *at;
                en.windup = None;
                f.target = None;
            }
        }
        let mut fights = world.resource_mut::<Fights>();
        fights.rewards.extend(rewards);
        fights.friends.extend(friends);
        fights.events.extend(fx);
    }

    /// Answered, resolved or timed out.
    fn settle(&mut self, world: &mut World, tick: u64, heroes: &[Seen]) {
        // The walking tank moves on: its call follows it.
        let tanks: Vec<(usize, DVec3)> = world
            .query::<(&combat::Enemy, &Motion)>()
            .iter(world)
            .filter(|(e, _)| e.kind == EnemyKind::Colossus)
            .map(|(e, m)| (e.spot, m.0.pos))
            .collect();
        for inc in self.open.iter_mut() {
            let Some(at) = inc
                .spot
                .and_then(|s| tanks.iter().find(|t| t.0 == s))
                .map(|t| t.1)
            else {
                continue;
            };
            if at.distance(inc.at) > 20.0 {
                inc.at = at;
                self.news.push(inc.news());
            }
        }
        // A fire under the weather tower's rain burns at half pace: the
        // people inside hold out longer.
        let wx = self.town.as_ref().and_then(|t| {
            t.world
                .landmarks
                .iter()
                .find(|l| l.kind == LandmarkKind::WeatherTower)
                .cloned()
        });
        if let Some(wx) = &wx {
            let now = tick as f64 * DT;
            for inc in self.open.iter_mut() {
                if inc.kind == IncidentKind::Fire
                    && crate::rainmaker::rain_at(wx, DVec2::new(inc.at.x, inc.at.z), now) > 0.5
                {
                    inc.expires += TICK_HZ as u64 / 2;
                }
            }
        }
        // Where each fight's villains are: (spot, place, beaten: lying
        // down or carried off, so the police have them wherever they are).
        let crooks: Vec<(usize, DVec3, bool)> = world
            .query::<(&combat::Enemy, &Motion, &Fighter)>()
            .iter(world)
            .map(|(e, m, f)| (e.spot, m.0.pos, !f.fighting()))
            .collect();
        let mut ended = Vec::new();
        let (mut making_off, mut caught_up) = (Vec::new(), Vec::new());
        for (k, inc) in self.open.iter_mut().enumerate() {
            if let Some(t) = inc.calmed {
                // Talked down: resolved once they are home.
                if tick >= t + ticks(HOME_SECONDS) {
                    inc.state = State::Resolved;
                    ended.push(k);
                }
                continue;
            }
            if heroes.iter().any(|h| h.1.distance(inc.at) < ANSWER) {
                inc.answered = true;
            }
            let nearest = heroes
                .iter()
                .map(|h| h.1.distance(inc.at))
                .fold(f64::INFINITY, f64::min);
            inc.watch(tick, nearest);
            let beaten = match inc.spot {
                Some(i) => {
                    let spot = &world.resource::<Fights>().spots[i];
                    // All gone without a fight is not beaten.
                    spot.groups > 0 && spot.members == 0 && (inc.escaped == 0 || inc.caught)
                }
                None => inc.left == 0 && inc.hurt.is_none(),
            };
            // Its villains still about? (A call with nobody there is a lie.)
            let (crew, by) = match inc.spot {
                Some(i) => {
                    let mine = crooks.iter().filter(|c| c.0 == i);
                    let by = mine
                        .clone()
                        .any(|c| c.2 || c.1.distance(inc.at) < STRAY_REACH);
                    (mine.count(), by)
                }
                None => (1, true),
            };
            if by {
                inc.strayed = None;
            } else if inc.strayed.is_none() {
                inc.strayed = Some(tick);
            }
            let got_away = crew == 0
                || inc
                    .strayed
                    .is_some_and(|t| tick >= t + ticks(STRAY_SECONDS));
            if beaten {
                inc.state = State::Resolved;
                ended.push(k);
            } else if let Some(t) = inc.fleeing {
                // Making off: gone once all are away (or after a while);
                // a hero who catches up makes it a fight again.
                let mine: Vec<DVec3> = crooks
                    .iter()
                    .filter(|c| Some(c.0) == inc.spot && !c.2)
                    .map(|c| c.1)
                    .collect();
                let caught = heroes
                    .iter()
                    .any(|h| mine.iter().any(|c| c.distance(h.1) < CATCH_UP));
                if crew == 0 || tick >= t + ticks(FLEE_MOST) {
                    inc.state = State::TimedOut;
                    ended.push(k);
                } else if caught {
                    caught_up.push(k);
                } else if !mine.is_empty() && !inc.kind.flees_to_its_ride() {
                    // The call follows them.
                    let mid = mine.iter().copied().sum::<DVec3>() / mine.len() as f64;
                    if mid.distance(inc.at) > 8.0 {
                        inc.at = mid;
                        self.news.push(inc.news());
                    }
                }
            } else if got_away {
                // Gone without being beaten (none left, or all far off):
                // the crooks got away.
                inc.state = State::TimedOut;
                ended.push(k);
            } else if tick >= inc.expires && !inc.answered {
                if inc.spot.is_some() && crew > 0 && inc.kind.makes_off() {
                    // Nobody came: they make off with it, in plain view.
                    making_off.push(k);
                } else {
                    inc.state = State::TimedOut;
                    ended.push(k);
                }
            } else if inc.answered && heroes.iter().all(|h| h.1.distance(inc.at) > combat::SLEEP) {
                // Answered, then left behind: they get away all the same.
                inc.state = State::TimedOut;
                ended.push(k);
            }
        }
        for k in making_off {
            self.make_off(world, tick, k, heroes);
        }
        for k in caught_up {
            self.catch_up(world, k);
        }
        for k in ended.into_iter().rev() {
            let inc = self.open.remove(k);
            release_bystanders(world, tick, inc.id);
            if let Some(spot) = inc.spot {
                match inc.state {
                    State::Resolved if inc.calmed.is_none() => {
                        self.reward(world, tick, &inc, heroes)
                    }
                    _ => Self::send_home(world, spot),
                }
                self.free.push(spot);
            }
            self.news.push(inc.news());
        }
    }

    fn reward(&self, world: &mut World, tick: u64, inc: &Incident, heroes: &[Seen]) {
        let (size, only, _) = inc.kind.group();
        let base = combat::enemy_spec(only.unwrap_or(EnemyKind::Brute)).karma;
        let mut f = world.resource_mut::<Fights>();
        for h in heroes.iter().filter(|h| h.1.distance(inc.at) < REWARD) {
            let one = progress::defeat_karma(base, inc.level, h.2) as f32;
            let karma = (one * size as f32 * inc.kind.bonus()).round() as u32;
            f.rewards.push(combat::Reward { hero: h.0, karma });
            // Won without wrecking the street: a little more, as a Guardian.
            let tally = |m: &BTreeMap<PlayerId, f64>| m.get(&h.4).copied().unwrap_or(0.0);
            let spent = tally(&self.collateral) - tally(&inc.collateral_from);
            if spent <= CAREFUL_COST {
                let bonus = ((karma as f32 * CAREFUL_BONUS).round() as u32).max(1);
                f.rewards.push(combat::Reward {
                    hero: h.0,
                    karma: bonus,
                });
                let mut e = combat::event(tick, FxKind::Careful, h.0, h.0, h.1);
                e.amount = bonus.min(u16::MAX as u32) as u16;
                f.events.push(e);
            }
        }
    }

    /// Open incident `k`, unanswered, is over: its villains make off with
    /// what they took (thieves away from the heroes, raiders back to their
    /// ladders or capsule, with the townsfolk they hold).
    fn make_off(&mut self, world: &mut World, tick: u64, k: usize, heroes: &[Seen]) {
        let solids = world.resource::<Solids>().0.clone();
        let inc = &mut self.open[k];
        let Some(spot) = inc.spot else {
            return;
        };
        inc.fleeing = Some(tick);
        let f = DVec3::new(inc.facing.x, 0.0, inc.facing.y).normalize_or(DVec3::X);
        let side = DVec3::new(f.z, 0.0, -f.x);
        // Away from the nearest hero (or on the way it faces).
        let away = heroes
            .iter()
            .min_by(|a, b| a.1.distance(inc.at).total_cmp(&b.1.distance(inc.at)))
            .map(|h| DVec2::new(inc.at.x - h.1.x, inc.at.z - h.1.z).normalize_or(inc.facing))
            .unwrap_or(inc.facing);
        let to = match inc.kind {
            IncidentKind::IronLanding => inc.at - f * 5.5,
            IncidentKind::PirateRaid => inc.at,
            _ => flee_point(&solids, inc.at, away),
        };
        let mut crew: Vec<(NetId, Entity, DVec3)> = world
            .query::<(Entity, &Net, &combat::Enemy, &Motion, &Fighter)>()
            .iter(world)
            .filter(|(_, _, en, _, f)| en.spot == spot && f.fighting())
            .map(|(e, n, _, m, _)| (n.0, e, m.0.pos))
            .collect();
        crew.sort_by_key(|c| c.0);
        let (what, carriers) = inc.kind.loot();
        let mut fx = Vec::new();
        for (j, (id, e, pos)) in crew.iter().enumerate() {
            // Spread out a little, so they do not run into each other.
            let spread = side * ((j as f64 - (crew.len() as f64 - 1.0) * 0.5) * 1.4);
            let ladder = match inc.kind {
                // Up the two rope ladders by turns.
                IncidentKind::PirateRaid => side * if j % 2 == 0 { -2.4 } else { 2.4 },
                _ => spread,
            };
            if let Some(mut en) = world.get_mut::<combat::Enemy>(*e) {
                en.home = to + ladder;
                en.leaving = true;
            }
            if j < carriers {
                let mut l = combat::event(tick, FxKind::Loot, *id, *id, *pos);
                l.amount = what;
                fx.push(l);
            }
        }
        // The legion takes its captives along.
        if inc.kind == IncidentKind::IronLanding {
            let id = inc.id;
            for mut b in world.query::<&mut Bystander>().iter_mut(world) {
                if b.incident == id && b.leaving.is_none() {
                    b.march = Some(to);
                }
            }
        }
        world.resource_mut::<Fights>().events.extend(fx);
        let news = self.open[k].news();
        self.news.push(news);
    }

    /// A hero caught up with incident `k`'s crooks as they made off: they
    /// stand and fight, and the captives stop.
    fn catch_up(&mut self, world: &mut World, k: usize) {
        let inc = &mut self.open[k];
        inc.fleeing = None;
        inc.caught = true;
        inc.answered = true;
        inc.strayed = None;
        let (spot, id) = (inc.spot, inc.id);
        for (mut en, m) in world
            .query::<(&mut combat::Enemy, &Motion)>()
            .iter_mut(world)
        {
            if Some(en.spot) == spot && en.leaving {
                en.leaving = false;
                en.home = m.0.pos;
            }
        }
        for mut b in world.query::<&mut Bystander>().iter_mut(world) {
            if b.incident == id {
                b.march = None;
            }
        }
        let news = self.open[k].news();
        self.news.push(news);
    }

    /// Every second: crooks making off who got where they were going, out
    /// of every hero's sight, are gone (a thief round the corner, a pirate
    /// up the ladder); one still in sight runs on.
    fn got_away(&mut self, world: &mut World, heroes: &[Seen]) {
        for inc in self.open.iter_mut().filter(|i| i.fleeing.is_some()) {
            let Some(spot) = inc.spot else {
                continue;
            };
            let ride = inc.kind.flees_to_its_ride();
            let mut there: Vec<(NetId, Entity, DVec3, DVec3)> = world
                .query::<(Entity, &Net, &combat::Enemy, &Motion, &Fighter)>()
                .iter(world)
                .filter(|(_, _, en, m, f)| {
                    en.spot == spot
                        && en.leaving
                        && f.fighting()
                        && DVec2::new(m.0.pos.x - en.home.x, m.0.pos.z - en.home.z).length()
                            < FLEE_HOME
                })
                .map(|(e, n, en, m, _)| (n.0, e, m.0.pos, en.home))
                .collect();
            there.sort_by_key(|t| t.0);
            for (id, e, pos, home) in there {
                let seen = heroes.iter().any(|h| h.1.distance(pos) < FLEE_SEEN);
                if seen && !ride {
                    // Runs on, the same way.
                    let on =
                        DVec2::new(home.x - inc.at.x, home.z - inc.at.z).normalize_or(inc.facing);
                    let solids = world.resource::<Solids>().0.clone();
                    if let Some(mut en) = world.get_mut::<combat::Enemy>(e) {
                        en.home = flee_point(&solids, pos, on);
                    }
                    continue;
                }
                let mut f = world.resource_mut::<Fights>();
                f.paths.remove(&id);
                f.spots[spot].members = f.spots[spot].members.saturating_sub(1);
                world.despawn(e);
                inc.escaped += 1;
            }
        }
    }

    /// Every second: mutant brutes on a rampage, and the lab's loose
    /// technicians, wreck the street round them while nobody stops them:
    /// one walks up to a lamp, a bin or a bench now and then and smashes it.
    pub(crate) fn vandalise(
        &mut self,
        world: &mut World,
        tick: u64,
        street: &mut crate::props::PropDamage,
    ) {
        for inc in self.open.iter_mut() {
            if !matches!(inc.kind, IncidentKind::Rampage | IncidentKind::Breakout)
                || inc.fleeing.is_some()
                || tick < inc.next_smash
            {
                continue;
            }
            let Some(spot) = inc.spot else {
                continue;
            };
            let mut idle: Vec<(NetId, Entity, DVec3, bool)> = world
                .query::<(Entity, &Net, &combat::Enemy, &Motion, &Fighter)>()
                .iter(world)
                .filter(|(_, _, en, _, f)| {
                    en.spot == spot && !en.leaving && f.fighting() && f.target.is_none()
                })
                .map(|(e, n, en, m, _)| (n.0, e, m.0.pos, en.kind.back_row()))
                .collect();
            idle.sort_by_key(|i| i.0);
            // The one by a prop it is not already at.
            let at = DVec2::new(inc.at.x, inc.at.z);
            let props = street.breakable_near(at, SMASH_REACH);
            let pick = idle.iter().find_map(|v| {
                let me = DVec2::new(v.2.x, v.2.z);
                props
                    .iter()
                    .min_by(|a, b| a.1.distance(me).total_cmp(&b.1.distance(me)))
                    .map(|p| (*v, *p))
            });
            let Some(((id, e, pos, ranged), (prop, foot))) = pick else {
                inc.next_smash = tick + ticks(SMASH_EVERY);
                continue;
            };
            let me = DVec2::new(pos.x, pos.z);
            let reach = if ranged { 12.0 } else { 1.8 };
            if me.distance(foot) <= reach + 1.0 {
                // There: a blow (or a ray), and it goes over.
                let hit_at = DVec3::new(foot.x, pos.y + 1.0, foot.y);
                let kind = if ranged { FxKind::Ray } else { FxKind::Punch };
                let wind = if ranged {
                    FxKind::RayWindup
                } else {
                    FxKind::PunchWindup
                };
                if street.smash(prop, me, tick) {
                    let mut f = world.resource_mut::<Fights>();
                    f.events.push(combat::event(tick, wind, id, id, hit_at));
                    f.events.push(combat::event(tick, kind, id, id, hit_at));
                }
                if let Some(mut en) = world.get_mut::<combat::Enemy>(e) {
                    en.home = DVec3::new(pos.x, pos.y, pos.z);
                }
                inc.next_smash = tick + ticks(SMASH_EVERY);
            } else if let Some(mut en) = world.get_mut::<combat::Enemy>(e) {
                // On its way to it (it stops a step short).
                let to = foot + (me - foot).normalize_or(DVec2::X) * (reach * 0.7);
                en.home = DVec3::new(to.x, pos.y, to.y);
            }
        }
    }

    /// The villains of an incident that ended unanswered leave.
    fn send_home(world: &mut World, spot: usize) {
        let mut gone: Vec<(NetId, Entity)> = world
            .query::<(Entity, &Net, &combat::Enemy)>()
            .iter(world)
            .filter(|(_, _, en)| en.spot == spot)
            .map(|(e, n, _)| (n.0, e))
            .collect();
        gone.sort_by_key(|g| g.0);
        for (id, e) in gone {
            world.resource_mut::<Fights>().paths.remove(&id);
            world.despawn(e);
        }
        let mut f = world.resource_mut::<Fights>();
        f.spots[spot].members = 0;
        f.spots[spot].respawn_at = None;
    }

    /// Starts one near a hero.
    fn start(&mut self, world: &mut World, tick: u64, heroes: &[Seen]) {
        let Some(town) = self.town.clone() else {
            return;
        };
        let seed = Seed(world.resource::<Fights>().seed)
            .child("incidents")
            .child(self.count as u64);
        self.count += 1;
        self.last_start = Some(tick);
        let mut rng = seed.rng();
        let &(_, hero, _, afoot, _) = rng.pick(heroes);
        let me = DVec2::new(hero.x, hero.z);
        // Near enough to run to for a hero on foot.
        let (near, far) = if afoot {
            (NEAREST_AFOOT, FARTHEST_AFOOT)
        } else {
            (NEAREST, FARTHEST)
        };
        let ring = |p: DVec2| (near..far).contains(&p.distance(me));
        let hot = self.is_hot(hero);
        let quarter = town.town.dystopia.as_ref();
        let held = |p: DVec2| quarter.is_some_and(|d| d.contains(p));
        // Labs near enough for a breakout, shops for a robbery.
        let fronts = |usage: BuildingUse| -> Vec<(DVec2, DVec2)> {
            town.buildings
                .iter()
                .filter(|b| b.site.usage == usage)
                .map(|b| {
                    let s = &b.site;
                    let front = s.origin + s.along * s.size.x * 0.5 - s.inward * 3.0;
                    (front, -s.inward)
                })
                .filter(|(p, _)| ring(*p))
                .collect()
        };
        let labs = fronts(BuildingUse::Lab);
        let shops: Vec<_> = fronts(BuildingUse::ShopsAndOffices)
            .into_iter()
            .filter(|(p, _)| !held(*p))
            .collect();
        let homes: Vec<_> = fronts(BuildingUse::Apartments)
            .into_iter()
            .chain(fronts(BuildingUse::House))
            .chain(fronts(BuildingUse::Tenement))
            .collect();
        let streets: Vec<(DVec2, DVec2)> = town
            .roads
            .iter()
            .filter(|r| !r.main)
            .flat_map(|r| r.pts.windows(2).map(|w| (w[0], (w[1] - w[0]).perp())))
            .filter(|(p, _)| ring(*p) && !held(*p))
            .collect();
        let mut kinds: Vec<(IncidentKind, f64)> = vec![
            // Round a gang's corner, mostly its muggings and hold-ups.
            (IncidentKind::Mugging, if hot { 0.6 } else { 0.25 }),
            (IncidentKind::Robbery, if hot { 0.3 } else { 0.2 }),
            // Brutes and lab breakouts are rarer there: they are fights
            // for heroes who have grown.
            (IncidentKind::Rampage, if hot { 0.05 } else { 0.15 }),
            (IncidentKind::Breakout, if hot { 0.04 } else { 0.12 }),
            (IncidentKind::CarCrash, 0.14),
            // Parked (Arnd 2026-11-10): rescuing people from a fire was the
            // car crash again. Only forced (HW_INCIDENT=fire, tests) until it
            // is redesigned as a city event the brigade handles; then give it
            // its weight back (was 0.14).
            (IncidentKind::Fire, 0.0),
            (IncidentKind::Colossus, 0.08),
            // Parked (Arnd 2026-11-10): the mole folk have no reason to
            // come up yet. Only forced (HW_INCIDENT=moles, tests) until the
            // event is redesigned; then give it its weight back (was 0.1).
            (IncidentKind::Surfacing, 0.0),
            (IncidentKind::PirateRaid, if hot { 0.05 } else { 0.1 }),
            (IncidentKind::IronLanding, 0.07),
            (IncidentKind::SaucerRaid, 0.07),
        ];
        // The walking tank marches out of the robot works' gate (or, in a
        // town without works, the Tesla tower's) now and then, once the team
        // can take it, and never two at once.
        let tower = town
            .landmarks
            .iter()
            .find(|l| l.kind == LandmarkKind::RobotWorks)
            .or_else(|| {
                town.landmarks
                    .iter()
                    .find(|l| l.kind == LandmarkKind::TeslaTower)
            })
            .map(|l| {
                // Out onto the nearest street wide enough for it.
                let front = l.pos + l.dir * (l.plaza + 14.0);
                let street = world
                    .resource::<Fights>()
                    .streets
                    .as_ref()
                    .and_then(|s| s.nearest(front).map(|n| s.node(n)))
                    .filter(|p| p.distance(front) < 120.0);
                match street {
                    Some(p) => (p, (p - l.pos).normalize_or(l.dir)),
                    None => (front, l.dir),
                }
            });
        let team = world.resource::<Fights>().team_level;
        let colossus_ok = tower.is_some()
            && !self.open.iter().any(|i| i.kind == IncidentKind::Colossus)
            && (self.next_kind == Some(IncidentKind::Colossus)
                || (team >= COLOSSUS_LEVEL
                    && self
                        .last_colossus
                        .is_none_or(|t| tick >= t + ticks(COLOSSUS_GAP))));
        let iron_ok = self.next_kind == Some(IncidentKind::IronLanding) || team >= IRON_LEVEL;
        let saucer_ok = self.next_kind == Some(IncidentKind::SaucerRaid) || team >= SAUCER_LEVEL;
        kinds.retain(|(k, _)| match k {
            IncidentKind::Breakout => !labs.is_empty(),
            IncidentKind::Robbery | IncidentKind::PirateRaid => !shops.is_empty(),
            IncidentKind::IronLanding => iron_ok && !streets.is_empty(),
            IncidentKind::SaucerRaid => saucer_ok && !streets.is_empty(),
            IncidentKind::Fire => self.next_kind == Some(IncidentKind::Fire) && !homes.is_empty(),
            IncidentKind::Colossus => colossus_ok,
            IncidentKind::Surfacing => {
                self.next_kind == Some(IncidentKind::Surfacing) && !streets.is_empty()
            }
            _ => !streets.is_empty(),
        });
        let total: f64 = kinds.iter().map(|k| k.1).sum();
        if total <= 0.0 {
            return;
        }
        let mut roll = rng.range(0.0, total);
        let mut kind = kinds[0].0;
        for (k, w) in &kinds {
            if roll < *w {
                kind = *k;
                break;
            }
            roll -= w;
        }
        if let Some(k) = self.next_kind.take() {
            if kinds.iter().any(|x| x.0 == k) {
                kind = k;
            }
        }
        let tower_place: Vec<(DVec2, DVec2)> = tower.into_iter().collect();
        let places = match kind {
            IncidentKind::Breakout => &labs,
            IncidentKind::Robbery | IncidentKind::PirateRaid => &shops,
            IncidentKind::Fire => &homes,
            IncidentKind::Colossus => &tower_place,
            _ => &streets,
        };
        // On foot, the nearer of two places: most of it round the corner.
        let (p, face) = {
            let a = *rng.pick(places);
            if afoot {
                let b = *rng.pick(places);
                if b.0.distance(me) < a.0.distance(me) {
                    b
                } else {
                    a
                }
            } else {
                a
            }
        };
        // Not on top of another one.
        if self
            .open
            .iter()
            .any(|i| DVec2::new(i.at.x, i.at.z).distance(p) < 60.0)
        {
            return;
        }
        let solids = world.resource::<Solids>().0.clone();
        let y = solids.ground_at(p).max(GROUND_Y);
        // On open ground: a fight's spot never sits in a house's wall.
        let p = if kind.trapped() > 0 {
            p
        } else {
            combat::open_stand(&solids, DVec3::new(p.x, y, p.y), p, &[])
        };
        let y = solids.ground_at(p).max(GROUND_Y);
        let at = DVec3::new(p.x, y, p.y);
        // A fight faces the hero who will come; a rescue the way its place
        // does (across the road, out of the house).
        let facing = if kind.trapped() > 0 {
            face.normalize_or(DVec2::X)
        } else {
            (me - p).normalize_or(face.normalize_or(DVec2::X))
        };
        let (size, only, extra) = kind.group();
        if kind.trapped() > 0 {
            let mut inc = self.rescue_at(world, tick, kind, at, facing);
            if std::mem::take(&mut self.carried_start) {
                let id = heroes
                    .iter()
                    .min_by(|a, b| a.1.distance(at).total_cmp(&b.1.distance(at)))
                    .map(|h| h.0);
                let mut q = world.query::<(&Net, &mut Motion)>();
                for (n, mut m) in q.iter_mut(world) {
                    if Some(n.0) == id {
                        m.0.pos = at + DVec3::new(facing.x, 0.0, facing.y) * 5.0 + DVec3::Y * 0.3;
                        m.0.vel = DVec3::ZERO;
                    }
                }
                if let Some(id) = id {
                    inc.left = 0;
                    inc.answered = true;
                    inc.hurt = Some(Hurt::Carried(id));
                }
            } else if std::mem::take(&mut self.beside_start) {
                let id = heroes
                    .iter()
                    .min_by(|a, b| a.1.distance(at).total_cmp(&b.1.distance(at)))
                    .map(|h| h.0);
                let next = inc.victims().last().copied().unwrap_or(at);
                // In front of them, on the street.
                let f = DVec3::new(facing.x, 0.0, facing.y);
                let out = if (next - at).dot(f) < 0.0 { -f } else { f };
                let mut q = world.query::<(&Net, &mut Motion)>();
                for (n, mut m) in q.iter_mut(world) {
                    if Some(n.0) == id {
                        m.0.pos = DVec3::new(next.x, at.y, next.z) + out * 1.6 + DVec3::Y * 0.3;
                        m.0.vel = DVec3::ZERO;
                    }
                }
            }
            self.news.push(inc.news());
            self.news.extend(inc.casualty(false));
            self.open.push(inc);
            return;
        }
        let spot = Spot {
            extra,
            size: Some(size),
            only,
            crew: kind.crew(),
            once: true,
            lead: kind.lead(),
            provoke: Some(kind.reach()),
            ..Spot::corner(at, facing)
        };
        let index = {
            let mut f = world.resource_mut::<Fights>();
            match self.free.pop() {
                Some(i) => {
                    // Keep the slot's count of groups: it varies the roster.
                    let groups = f.spots[i].groups;
                    f.spots[i] = Spot { groups, ..spot };
                    i
                }
                None => {
                    f.spots.push(spot);
                    f.spots.len() - 1
                }
            }
        };
        combat::spawn_group(world, tick, index, heroes.len());
        let level = world
            .resource::<Fights>()
            .team_level
            .saturating_add(extra)
            .min(progress::MAX_LEVEL);
        let inc = Incident {
            id: self.count,
            kind,
            at,
            spot: Some(index),
            facing,
            // Mole folk: 1 while they are frightened, 0 once talked down.
            left: (kind == IncidentKind::Surfacing) as u8,
            freeing: 0.0,
            hurt: None,
            medic: None,
            level,
            started: tick,
            expires: tick + ticks(self.timeout.unwrap_or(kind.timeout())),
            answered: false,
            state: State::Open,
            calmed: None,
            nearest: f64::INFINITY,
            coming_until: 0,
            doused: false,
            collateral_from: self.collateral.clone(),
            held_sent: 0,
            strayed: None,
            fleeing: None,
            escaped: 0,
            caught: false,
            next_smash: 0,
        };
        if kind == IncidentKind::Colossus {
            self.last_colossus = Some(tick);
        }
        if kind == IncidentKind::Mugging {
            spawn_bystander(world, tick, inc.id, mugged(at, facing), -facing, None);
        }
        for p in captives(kind, at, facing) {
            spawn_bystander(world, tick, inc.id, p, facing, None);
        }
        self.news.push(inc.news());
        self.open.push(inc);
    }

    /// Every open incident, for a player who joins.
    pub fn snapshot(&self) -> Vec<ToClient> {
        let mut all: Vec<ToClient> = self.open.iter().map(Incident::news).collect();
        all.extend(self.open.iter().filter_map(|i| i.casualty(false)));
        all
    }

    pub fn take_news(&mut self) -> Vec<ToClient> {
        std::mem::take(&mut self.news)
    }
}

/// Where crooks making off from `from` run to: about [`FLEE_FAR`] along
/// `dir` (turned aside as little as will do) to a place on the open
/// ground; the villains' own path finding takes them round what is in the
/// way.
fn flee_point(solids: &crate::collision::Obstacles, from: DVec3, dir: DVec2) -> DVec3 {
    let open = |p: DVec2| {
        let y = solids.ground_for(p, from.y).max(GROUND_Y);
        let at = DVec3::new(p.x, y, p.y);
        ((y - from.y).abs() < 4.0
            && solids.passable_at(at + DVec3::Y * 0.4)
            && solids.passable_at(at + DVec3::Y * 1.4))
        .then_some(at)
    };
    let here = DVec2::new(from.x, from.z);
    for far in [FLEE_FAR, FLEE_FAR * 0.6, FLEE_FAR * 0.35] {
        for a in [0.0, 25.0, -25.0, 50.0, -50.0, 80.0, -80.0, 120.0, -120.0] {
            let (s, c) = (det::sin(f64::to_radians(a)), det::cos(f64::to_radians(a)));
            let d = DVec2::new(dir.x * c - dir.y * s, dir.x * s + dir.y * c);
            if let Some(p) = open(here + d * far) {
                return p;
            }
        }
    }
    from
}

/// Where an ambulance waits for a rescue at `p`: parked at the kerb of a
/// street about [`MEDIC_AWAY`] off, along it. Only straight stretches, clear
/// of junctions, so it stands square with the kerb and not on a corner.
fn medic_spot(town: &Area, solids: &crate::collision::Obstacles, p: DVec2) -> (DVec3, DVec2) {
    use hw_gen::water::segment_distance;
    let quarter = town.town.dystopia.as_ref();
    // Every 4 m along every road: where, which way, how far off we'd like.
    let mut spots: Vec<(f64, DVec2, DVec2, usize)> = Vec::new();
    for (ri, r) in town.roads.iter().enumerate() {
        let len: f64 = r.pts.windows(2).map(|w| w[0].distance(w[1])).sum();
        let at = |s: f64| -> Option<(DVec2, DVec2)> {
            let mut s = s;
            for w in r.pts.windows(2) {
                let l = w[0].distance(w[1]);
                if s <= l && l > 1e-6 {
                    let d = (w[1] - w[0]) / l;
                    return Some((w[0] + d * s, d));
                }
                s -= l;
            }
            None
        };
        let mut s = MEDIC_CLEAR;
        while s <= len - MEDIC_CLEAR {
            let here = at(s);
            let (back, ahead) = (at(s - 9.0), at(s + 9.0));
            s += 4.0;
            let (Some((c, d)), Some((_, db)), Some((_, da))) = (here, back, ahead) else {
                continue;
            };
            // Straight for the ambulance's length and a little more.
            if d.dot(db) < 0.995 || d.dot(da) < 0.995 {
                continue;
            }
            let kerb = c + d.perp() * (r.half - 1.6).max(0.0);
            if quarter.is_some_and(|q| q.contains(kerb)) {
                continue;
            }
            spots.push(((kerb.distance(p) - MEDIC_AWAY).abs(), kerb, d, ri));
        }
    }
    spots.sort_by(|a, b| a.0.total_cmp(&b.0));
    // The nearest fit with no other road joining beside it.
    let clear = |kerb: DVec2, own: usize| {
        town.roads.iter().enumerate().all(|(i, r)| {
            i == own
                || r.pts
                    .windows(2)
                    .all(|w| segment_distance(kerb, w[0], w[1]) > r.half + MEDIC_CLEAR * 0.5)
        })
    };
    let pick = spots.iter().find(|s| clear(s.1, s.3)).or(spots.first());
    let (at, along) = pick.map_or((p + DVec2::X * 20.0, DVec2::X), |b| (b.1, b.2));
    let y = solids.ground_at(at).max(GROUND_Y);
    (DVec3::new(at.x, y, at.y), along)
}
