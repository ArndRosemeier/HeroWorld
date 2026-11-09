//! Messages between game clients and the server.
//!
//! Every packet starts with a 4-byte magic, the protocol version and a message
//! type. The server sends one [`ServerMsg::Update`] per tick and player: the
//! entities near that player that changed since the player last confirmed
//! them, the ones that went away, and reliable messages (joins, chat, the end
//! of a session) that are repeated until the other side confirms them.
//! Clients send their newest inputs (a few of the previous ones too, so one
//! lost packet loses no key press) and say which updates arrived.

use crate::wire::{Reader, WireError, Writer};
use crate::PROTOCOL_VERSION;
use hw_core::NetId;

const MAGIC: [u8; 4] = *b"HWLD";

/// Biggest UDP payload we send (stays under typical MTU).
pub const MAX_PACKET: usize = 1200;

/// Inputs repeated in every input packet (the newest last).
pub const INPUT_REDUNDANCY: usize = 4;

/// A player, numbered by the server from 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlayerId(pub u16);

/// What a player is pressing, sampled once per client tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InputState {
    /// Wanted horizontal movement in the local frame (x east, z south), length ≤ 1.
    pub move_x: f32,
    pub move_z: f32,
    /// Up or down while flying; (move_x, move_y, move_z) has length ≤ 1.
    pub move_y: f32,
    pub jump: bool,
    /// Go down (in a lift or in flight).
    pub descend: bool,
    /// Flying (the client keeps the toggle and sends it every tick, so a lost
    /// packet cannot lose a key press).
    pub fly: bool,
    /// Faster flight.
    pub boost: bool,
    /// Punch what is in front of the hero (held: again and again).
    pub smash: bool,
    /// Fire an energy blast where the hero aims.
    pub blast: bool,
    /// Where the hero aims (unit vector in the local frame): the camera's
    /// direction. Punches and blasts go this way.
    pub aim: [f32; 3],
    /// Another power used this tick ([`Power::code`], 0 for none): hold,
    /// shield, lift and throw, shockwave. Strike and bolt come as `smash`
    /// and `blast`.
    pub power: u8,
    /// The enemy the player has picked (Tab), if any.
    pub target: Option<NetId>,
    /// Holding the revive key next to a downed teammate.
    pub revive: bool,
    /// A downed hero calls the medical drone now instead of waiting.
    pub hospital: bool,
}

/// Every power a hero can have (see `hw_sim::progress` for what each does).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Power {
    /// A punch: free, weak, close. Every hero has it.
    Strike,
    /// An energy bolt: costs energy, reaches far, knocks back.
    Bolt,
    /// Jumps several storeys, with control in the air.
    SuperJump,
    /// Binds a villain where it stands for a few seconds.
    Hold,
    /// A shield that halves the damage to the hero and friends near.
    Shield,
    /// Picks up a crate (later cars and the like) and throws it.
    Lift,
    /// Flight.
    Flight,
    /// A blast all round that throws villains off their feet.
    Shockwave,
    /// Tier 4: runs several times as fast on the ground (movement).
    SuperSpeed,
    /// Tier 4: heals the hero and friends near.
    Mend,
    /// Tier 5: a ray that goes through every villain in its line.
    Beam,
    /// Tier 5, passive: health comes back quickly, even in a fight.
    Vitality,
    /// Tier 6: calls a strike from the sky down on a spot.
    Meteor,
    /// Tier 6, passive: every blow does less.
    Invulnerable,
    /// Tier 7: a huge blast all round that spends most of the hero's energy.
    Nova,
    /// Tier 7, passive: the hero and friends near hit harder and get their
    /// energy back sooner.
    Rally,
}

impl Power {
    pub const ALL: [Power; 16] = [
        Power::Strike,
        Power::Bolt,
        Power::SuperJump,
        Power::Hold,
        Power::Shield,
        Power::Lift,
        Power::Flight,
        Power::Shockwave,
        Power::SuperSpeed,
        Power::Mend,
        Power::Beam,
        Power::Vitality,
        Power::Meteor,
        Power::Invulnerable,
        Power::Nova,
        Power::Rally,
    ];

    pub fn code(self) -> u8 {
        match self {
            Power::Strike => 1,
            Power::Bolt => 2,
            Power::SuperJump => 3,
            Power::Hold => 4,
            Power::Shield => 5,
            Power::Lift => 6,
            Power::Flight => 7,
            Power::Shockwave => 8,
            Power::SuperSpeed => 9,
            Power::Mend => 10,
            Power::Beam => 11,
            Power::Vitality => 12,
            Power::Meteor => 13,
            Power::Invulnerable => 14,
            Power::Nova => 15,
            Power::Rally => 16,
        }
    }

    pub fn from_code(c: u8) -> Option<Self> {
        Power::ALL.into_iter().find(|p| p.code() == c)
    }
}

/// The world's factions: townsfolk and the people who keep or break the
/// peace. A hero belongs to none; each faction regards a hero the way it
/// regards townsfolk until the hero's deeds change that (see
/// `hw_sim::factions`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Faction {
    /// Ordinary people of the town.
    Townsfolk,
    /// The City Patrol: officers and police drones.
    Patrol,
    /// The mad scientist's lab: mutants, technicians, robots.
    Lab,
    /// The Chrome Jackets, a hot-rod street gang.
    Jackets,
    /// The Velvet Syndicate, pinstriped racketeers.
    Syndicate,
    /// Molehaven's people, under the city.
    Moles,
    /// The sky pirates of the airship *Cloudcutter*: buccaneers who raid
    /// rooftops and moor over the harbour.
    SkyPirates,
    /// The Iron-Masked Empire: invaders from beyond the sea who drop from
    /// the sky in iron capsules and take townsfolk away.
    Empire,
}

/// How many factions there are.
pub const FACTIONS: usize = 8;

impl Faction {
    pub const ALL: [Faction; FACTIONS] = [
        Faction::Townsfolk,
        Faction::Patrol,
        Faction::Lab,
        Faction::Jackets,
        Faction::Syndicate,
        Faction::Moles,
        Faction::SkyPirates,
        Faction::Empire,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Faction::Townsfolk => "Townsfolk",
            Faction::Patrol => "City Patrol",
            Faction::Lab => "The Doctor's Lab",
            Faction::Jackets => "Chrome Jackets",
            Faction::Syndicate => "Velvet Syndicate",
            Faction::Moles => "Molehaven",
            Faction::SkyPirates => "Sky Pirates",
            Faction::Empire => "Iron-Masked Empire",
        }
    }
}

/// What an entity is, so the client knows how to draw it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EntityKind {
    /// A player's hero.
    #[default]
    Hero,
    /// A loose crate: a physics body anyone can shove around.
    Crate,
    /// The mad scientist's mutant brute: a melee rusher.
    Brute,
    /// The mad scientist's ray-gun technician: shoots from a distance.
    Technician,
    /// A drone that lifts the defeated away (police for villains, medical
    /// for heroes).
    Drone,
    /// A flying taxi; its `owner` is the fare.
    Taxi,
    /// The mad scientist's robo-sentry: a lieutenant with a ray cannon and
    /// a shield projector on its back.
    Sentry,
    /// A mutant alpha: a boss brute that leaps onto heroes.
    Alpha,
    /// The walking tank: the colossus.
    Colossus,
    /// A weak point on a robot (a knee, a cannon, a shield dish); its
    /// `mount` says which robot and which part.
    WeakPoint,
    /// A Chrome Jacket: a street-gang greaser who fights with fists and a
    /// tyre iron.
    Greaser,
    /// A Velvet Syndicate gunsel: pinstripes, fedora and a ray pistol.
    Gunsel,
    /// A townsperson caught up in a crime (being mugged): drawn as a
    /// citizen; heroes cannot target them.
    Bystander,
    /// One of Molehaven's delvers: a mole-folk tunneller with a pick.
    Delver,
    /// A sky pirate buccaneer: a cutlass and a striped shirt.
    Buccaneer,
    /// A sky pirate gunner: a long coat and a ray pistol.
    Gunner,
    /// A sky pirate captain: a lieutenant with a cutlass.
    Captain,
    /// An Iron-Masked legionary: an iron helm and a ray rifle.
    Legionary,
    /// An Iron-Masked praetorian: a crested helm, armour and a shock lance;
    /// a lieutenant.
    Praetorian,
    /// An Aether Tube capsule; its `owner` is the passenger.
    Capsule,
    /// One of the town's own heroes from Hero Hall (`mount`: its own ID
    /// and which of them it is).
    NpcHero,
}

impl EntityKind {
    fn code(self) -> u8 {
        match self {
            EntityKind::Hero => 0,
            EntityKind::Crate => 1,
            EntityKind::Brute => 2,
            EntityKind::Technician => 3,
            EntityKind::Drone => 4,
            EntityKind::Taxi => 5,
            EntityKind::Sentry => 6,
            EntityKind::Alpha => 7,
            EntityKind::Colossus => 8,
            EntityKind::WeakPoint => 9,
            EntityKind::Greaser => 10,
            EntityKind::Gunsel => 11,
            EntityKind::Bystander => 12,
            EntityKind::Delver => 13,
            EntityKind::Buccaneer => 14,
            EntityKind::Gunner => 15,
            EntityKind::Captain => 16,
            EntityKind::Legionary => 17,
            EntityKind::Praetorian => 18,
            EntityKind::Capsule => 19,
            EntityKind::NpcHero => 20,
        }
    }

    fn from_code(c: u8) -> Result<Self, WireError> {
        match c {
            0 => Ok(EntityKind::Hero),
            1 => Ok(EntityKind::Crate),
            2 => Ok(EntityKind::Brute),
            3 => Ok(EntityKind::Technician),
            4 => Ok(EntityKind::Drone),
            5 => Ok(EntityKind::Taxi),
            6 => Ok(EntityKind::Sentry),
            7 => Ok(EntityKind::Alpha),
            8 => Ok(EntityKind::Colossus),
            9 => Ok(EntityKind::WeakPoint),
            10 => Ok(EntityKind::Greaser),
            11 => Ok(EntityKind::Gunsel),
            12 => Ok(EntityKind::Bystander),
            13 => Ok(EntityKind::Delver),
            14 => Ok(EntityKind::Buccaneer),
            15 => Ok(EntityKind::Gunner),
            16 => Ok(EntityKind::Captain),
            17 => Ok(EntityKind::Legionary),
            18 => Ok(EntityKind::Praetorian),
            19 => Ok(EntityKind::Capsule),
            20 => Ok(EntityKind::NpcHero),
            _ => Err(WireError),
        }
    }
}

impl EntityKind {
    /// Villains a hero fights.
    pub fn is_enemy(self) -> bool {
        matches!(
            self,
            EntityKind::Brute
                | EntityKind::Technician
                | EntityKind::Sentry
                | EntityKind::Alpha
                | EntityKind::Colossus
                | EntityKind::WeakPoint
                | EntityKind::Greaser
                | EntityKind::Gunsel
                | EntityKind::Delver
                | EntityKind::Buccaneer
                | EntityKind::Gunner
                | EntityKind::Captain
                | EntityKind::Legionary
                | EntityKind::Praetorian
        )
    }

    /// Shoots from a distance (draws a ray gun).
    pub fn is_ranged(self) -> bool {
        matches!(
            self,
            EntityKind::Technician
                | EntityKind::Gunsel
                | EntityKind::Gunner
                | EntityKind::Legionary
        )
    }

    /// Fights hand to hand with a weapon it carries (a pick, a cutlass, a
    /// lance).
    pub fn wields(self) -> bool {
        matches!(
            self,
            EntityKind::Delver
                | EntityKind::Buccaneer
                | EntityKind::Captain
                | EntityKind::Praetorian
        )
    }

    /// Drawn as a machine (prefab parts), not as a person.
    pub fn is_robot(self) -> bool {
        matches!(
            self,
            EntityKind::Sentry | EntityKind::Colossus | EntityKind::WeakPoint
        )
    }
}

/// Where a fighter is in a fight.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Status {
    /// On its feet.
    #[default]
    Standing,
    /// Knocked back: tumbling as a ragdoll, then getting up.
    Knocked,
    /// Defeated: a villain lies cuffed in energy bonds, a hero waits for a
    /// teammate or the medical drone.
    Down,
    /// Lifted away by a drone.
    Carried,
}

impl Status {
    fn code(self) -> u8 {
        match self {
            Status::Standing => 0,
            Status::Knocked => 1,
            Status::Down => 2,
            Status::Carried => 3,
        }
    }

    fn from_code(c: u8) -> Result<Self, WireError> {
        Ok(match c {
            0 => Status::Standing,
            1 => Status::Knocked,
            2 => Status::Down,
            3 => Status::Carried,
            _ => return Err(WireError),
        })
    }
}

/// A power a hero owns: which, in which theme (its look: 0 or 1), and
/// the karma paid for it (90% comes back if it is given up).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnedPower {
    pub power: Power,
    pub theme: u8,
    pub paid: u32,
}

/// A hero's progress: what the hero file on the player's PC holds and the
/// server plays by (see `hw_sim::progress` for the rules).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sheet {
    /// All karma ever earned: sets the level.
    pub lifetime: u32,
    /// Karma not spent yet: buys powers.
    pub karma: u32,
    /// Leaning per affinity (Guardian, Justice, Science, Mystic, Might).
    pub affinity: [u32; 5],
    /// The origin picked at creation (an index into `hw_sim::progress::ORIGINS`).
    pub origin: u8,
    /// Powers bought, in order (Strike is everyone's and not listed).
    pub powers: Vec<OwnedPower>,
    /// How each faction's regard for the hero has moved from what it has
    /// for townsfolk (by [`Faction::index`]): the hero's deeds.
    pub regard: [i8; FACTIONS],
}

/// Most powers a sheet carries over the wire.
pub const MAX_POWERS: usize = 32;

impl Sheet {
    pub fn wire_size(&self) -> usize {
        4 + 4 + 20 + 1 + FACTIONS + 1 + 6 * self.powers.len().min(MAX_POWERS)
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.lifetime).u32(self.karma);
        for a in self.affinity {
            w.u32(a);
        }
        w.u8(self.origin);
        for g in self.regard {
            w.u8(g as u8);
        }
        let n = self.powers.len().min(MAX_POWERS);
        w.u8(n as u8);
        for p in &self.powers[..n] {
            w.u8(p.power.code()).u8(p.theme).u32(p.paid);
        }
    }

    pub fn read(r: &mut Reader) -> Result<Self, WireError> {
        let lifetime = r.u32()?;
        let karma = r.u32()?;
        let mut affinity = [0u32; 5];
        for a in affinity.iter_mut() {
            *a = r.u32()?;
        }
        let origin = r.u8()?;
        let mut regard = [0i8; FACTIONS];
        for g in regard.iter_mut() {
            *g = r.u8()? as i8;
        }
        let n = r.u8()? as usize;
        if n > MAX_POWERS {
            return Err(WireError);
        }
        let mut powers = Vec::with_capacity(n);
        for _ in 0..n {
            let power = Power::from_code(r.u8()?).ok_or(WireError)?;
            powers.push(OwnedPower {
                power,
                theme: r.u8()?,
                paid: r.u32()?,
            });
        }
        Ok(Self {
            lifetime,
            karma,
            affinity,
            origin,
            powers,
            regard,
        })
    }

    /// Whether the hero owns this power (Strike: always).
    pub fn has(&self, power: Power) -> bool {
        power == Power::Strike || self.powers.iter().any(|p| p.power == power)
    }

    /// The theme the hero picked for a power (0 if not owned).
    pub fn theme(&self, power: Power) -> u8 {
        self.powers
            .iter()
            .find(|p| p.power == power)
            .map_or(0, |p| p.theme)
    }
}

/// A fighter's state as clients see it: bars in 0..=255, so small changes
/// (regeneration) cost nothing until they show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CombatState {
    pub health: u8,
    pub energy: u8,
    pub status: Status,
    pub level: u8,
    /// Who it fights (an enemy's foe, a hero's picked target).
    pub target: Option<NetId>,
    /// What powers do to it now ([`EFFECT_HELD`], [`EFFECT_SHIELD`], ...).
    pub effects: u16,
}

/// Bound by a binding hold.
pub const EFFECT_HELD: u16 = 1;
/// Behind a force shield.
pub const EFFECT_SHIELD: u16 = 2;
/// Carrying something over its head.
pub const EFFECT_CARRYING: u16 = 4;
/// Flying is unlocked (heroes with the Flight power).
pub const EFFECT_CAN_FLY: u16 = 8;
/// Super jump is owned (higher jumps).
pub const EFFECT_SUPER_JUMP: u16 = 16;
/// A robot's armour is shut: blows glance off (the colossus walking).
pub const EFFECT_ARMOURED: u16 = 32;
/// The colossus is down on its belly, its core open.
pub const EFFECT_KNEELING: u16 = 64;
/// The colossus is enraged (its second wind).
pub const EFFECT_ENRAGED: u16 = 128;
/// Super speed is owned (fast running on the ground).
pub const EFFECT_SUPER_SPEED: u16 = 256;
/// Inside a rallying hero's aura (hits harder; a glow).
pub const EFFECT_RALLIED: u16 = 512;
/// Owns Invulnerable (blows glance off; a sheen).
pub const EFFECT_TOUGH: u16 = 1024;

/// Bytes a [`CombatState`] takes.
const COMBAT_BYTES: usize = 10;

/// No rotation.
pub const NO_ROTATION: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// One entity as the server sends it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityState {
    pub id: NetId,
    pub kind: EntityKind,
    /// Position in the city's local frame (f64: precise anywhere in the city).
    pub pos: [f64; 3],
    pub vel: [f32; 3],
    /// Orientation as a quaternion (x, y, z, w); heroes stay upright.
    pub rot: [f32; 4],
    /// Standing on something (needed to replay movement exactly).
    pub grounded: bool,
    /// The player who controls it, if it is a hero.
    pub owner: Option<PlayerId>,
    /// Health, energy and status, for anything that fights.
    pub combat: Option<CombatState>,
    /// A weak point's robot and which part of it it is.
    pub mount: Option<(NetId, u8)>,
}

impl Default for EntityState {
    fn default() -> Self {
        Self {
            id: NetId(0),
            kind: EntityKind::Hero,
            pos: [0.0; 3],
            vel: [0.0; 3],
            rot: NO_ROTATION,
            grounded: false,
            owner: None,
            combat: None,
            mount: None,
        }
    }
}

const F_GROUNDED: u8 = 1;
const F_OWNER: u8 = 2;
const F_VEL: u8 = 4;
const F_ROT: u8 = 8;
const F_COMBAT: u8 = 16;
const F_MOUNT: u8 = 32;

impl EntityState {
    fn flags(&self) -> u8 {
        let mut f = 0;
        if self.grounded {
            f |= F_GROUNDED;
        }
        if self.owner.is_some() {
            f |= F_OWNER;
        }
        if self.vel != [0.0; 3] {
            f |= F_VEL;
        }
        if self.rot != NO_ROTATION {
            f |= F_ROT;
        }
        if self.combat.is_some() {
            f |= F_COMBAT;
        }
        if self.mount.is_some() {
            f |= F_MOUNT;
        }
        f
    }

    /// Bytes this entity takes in an update.
    pub fn wire_size(&self) -> usize {
        let f = self.flags();
        4 + 1
            + 1
            + 24
            + if f & F_VEL != 0 { 12 } else { 0 }
            + if f & F_ROT != 0 { 16 } else { 0 }
            + if f & F_OWNER != 0 { 2 } else { 0 }
            + if f & F_COMBAT != 0 { COMBAT_BYTES } else { 0 }
            + if f & F_MOUNT != 0 { 5 } else { 0 }
    }

    fn write(&self, w: &mut Writer) {
        let f = self.flags();
        w.u32(self.id.0).u8(self.kind.code()).u8(f);
        for v in self.pos {
            w.f64(v);
        }
        if f & F_VEL != 0 {
            for v in self.vel {
                w.f32(v);
            }
        }
        if f & F_ROT != 0 {
            for v in self.rot {
                w.f32(v);
            }
        }
        if let Some(p) = self.owner {
            w.u16(p.0);
        }
        if let Some(c) = self.combat {
            w.u8(c.health)
                .u8(c.energy)
                .u8(c.status.code())
                .u8(c.level)
                .u32(c.target.map_or(0, |t| t.0))
                .u16(c.effects);
        }
        if let Some((on, part)) = self.mount {
            w.u32(on.0).u8(part);
        }
    }

    fn read(r: &mut Reader) -> Result<Self, WireError> {
        let id = NetId(r.u32()?);
        let kind = EntityKind::from_code(r.u8()?)?;
        let f = r.u8()?;
        if f >> 6 != 0 {
            return Err(WireError);
        }
        let pos = [r.f64()?, r.f64()?, r.f64()?];
        let vel = if f & F_VEL != 0 {
            [r.f32()?, r.f32()?, r.f32()?]
        } else {
            [0.0; 3]
        };
        let rot = if f & F_ROT != 0 {
            [r.f32()?, r.f32()?, r.f32()?, r.f32()?]
        } else {
            NO_ROTATION
        };
        let owner = if f & F_OWNER != 0 {
            match r.u16()? {
                0 => return Err(WireError),
                p => Some(PlayerId(p)),
            }
        } else {
            None
        };
        let combat = if f & F_COMBAT != 0 {
            Some(CombatState {
                health: r.u8()?,
                energy: r.u8()?,
                status: Status::from_code(r.u8()?)?,
                level: r.u8()?,
                target: match r.u32()? {
                    0 => None,
                    t => Some(NetId(t)),
                },
                effects: r.u16()?,
            })
        } else {
            None
        };
        let mount = if f & F_MOUNT != 0 {
            Some((NetId(r.u32()?), r.u8()?))
        } else {
            None
        };
        Ok(Self {
            id,
            kind,
            pos,
            vel,
            rot,
            grounded: f & F_GROUNDED != 0,
            owner,
            mount,
            combat,
        })
    }
}

/// Something that happened in a fight, for clients to show (sparks, beams,
/// sounds, shakes, the floating numbers). The server decides; this only
/// tells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FxKind {
    /// A hero's punch landed.
    Strike,
    /// A hero's energy bolt hit.
    Bolt,
    /// A brute winds up a punch (the telegraph).
    PunchWindup,
    /// A brute's punch landed.
    Punch,
    /// A technician aims its ray gun (the telegraph).
    RayWindup,
    /// A technician's ray hit.
    Ray,
    /// A downed hero is back on their feet (a teammate revived them).
    Revived,
    /// A drone has picked someone up.
    Lifted,
    /// A hero came back from hospital.
    Healed,
    /// A binding hold took (`to` is held).
    Hold,
    /// A force shield went up round `to`.
    Shield,
    /// A shockwave went off round `from` (`amount`: villains caught).
    Shockwave,
    /// `from` lifted something over its head (`to`: what).
    Lift,
    /// `from` threw what it carried at `to`.
    Throw,
    /// A thrown thing hit `to`.
    ThrowHit,
    /// `to` reached a new level (`amount`).
    LevelUp,
    /// `to` earned karma (`amount`).
    Karma,
    /// The colossus aims its cannon: a shell will land at `at`.
    ShellWindup,
    /// A shell landed at `at` (`to`: a hero it hit, or `from`).
    Shell,
    /// The colossus rears up to stomp round `at`.
    StompWindup,
    /// The stomp's shockwave (`to`: a hero it hit, or `from`).
    Stomp,
    /// The core charges: an Aether discharge round `at` is coming.
    PulseWindup,
    /// The discharge (`to`: a hero it hit, or `from`).
    Pulse,
    /// A mutant alpha crouches to leap onto `at`.
    LeapWindup,
    /// It landed (`to`: a hero it hit, or `from`).
    Leap,
    /// A robot part was blown off (`to`: the weak point).
    PartBroken,
    /// The colossus stands up again (its second wind) or goes down.
    Rise,
    Kneel,
    /// A villain warns a hero off its turf (`from` warns `to`).
    Warn,
    /// A hero's mend healed `to` (`amount`: health given back).
    Mend,
    /// A beam went from `from` toward `at` (its far end); on a villain it
    /// hit, `to` is that villain.
    Beam,
    /// A strike from the sky is called down on `at` (lands in a moment).
    MeteorCall,
    /// It landed at `at` (`to`: a villain it hit, or `from` for the blast).
    Meteor,
    /// A nova went off round `from` (`to`: a villain it hit, or `from`).
    Nova,
    /// Frightened people a hero talked down thank them and go home (`from`
    /// speaks to `to`).
    Calmed,
    /// One of the town's heroes gives a newcomer a tip (`from` speaks to
    /// `to`; `amount`: which tip).
    Tip,
}

impl FxKind {
    fn code(self) -> u8 {
        match self {
            FxKind::Strike => 1,
            FxKind::Bolt => 2,
            FxKind::PunchWindup => 3,
            FxKind::Punch => 4,
            FxKind::RayWindup => 5,
            FxKind::Ray => 6,
            FxKind::Revived => 7,
            FxKind::Lifted => 8,
            FxKind::Healed => 9,
            FxKind::Hold => 10,
            FxKind::Shield => 11,
            FxKind::Shockwave => 12,
            FxKind::Lift => 13,
            FxKind::Throw => 14,
            FxKind::LevelUp => 15,
            FxKind::Karma => 16,
            FxKind::ThrowHit => 17,
            FxKind::ShellWindup => 18,
            FxKind::Shell => 19,
            FxKind::StompWindup => 20,
            FxKind::Stomp => 21,
            FxKind::PulseWindup => 22,
            FxKind::Pulse => 23,
            FxKind::LeapWindup => 24,
            FxKind::Leap => 25,
            FxKind::PartBroken => 26,
            FxKind::Rise => 27,
            FxKind::Kneel => 28,
            FxKind::Warn => 29,
            FxKind::Mend => 30,
            FxKind::Beam => 31,
            FxKind::MeteorCall => 32,
            FxKind::Meteor => 33,
            FxKind::Nova => 34,
            FxKind::Calmed => 35,
            FxKind::Tip => 36,
        }
    }

    fn from_code(c: u8) -> Result<Self, WireError> {
        Ok(match c {
            1 => FxKind::Strike,
            2 => FxKind::Bolt,
            3 => FxKind::PunchWindup,
            4 => FxKind::Punch,
            5 => FxKind::RayWindup,
            6 => FxKind::Ray,
            7 => FxKind::Revived,
            8 => FxKind::Lifted,
            9 => FxKind::Healed,
            10 => FxKind::Hold,
            11 => FxKind::Shield,
            12 => FxKind::Shockwave,
            13 => FxKind::Lift,
            14 => FxKind::Throw,
            15 => FxKind::LevelUp,
            16 => FxKind::Karma,
            17 => FxKind::ThrowHit,
            18 => FxKind::ShellWindup,
            19 => FxKind::Shell,
            20 => FxKind::StompWindup,
            21 => FxKind::Stomp,
            22 => FxKind::PulseWindup,
            23 => FxKind::Pulse,
            24 => FxKind::LeapWindup,
            25 => FxKind::Leap,
            26 => FxKind::PartBroken,
            27 => FxKind::Rise,
            28 => FxKind::Kneel,
            29 => FxKind::Warn,
            30 => FxKind::Mend,
            31 => FxKind::Beam,
            32 => FxKind::MeteorCall,
            33 => FxKind::Meteor,
            34 => FxKind::Nova,
            35 => FxKind::Calmed,
            36 => FxKind::Tip,
            _ => return Err(WireError),
        })
    }
}

/// One fight event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fx {
    /// The server tick it happened on (clients drop effects that arrive late).
    pub tick: u64,
    pub kind: FxKind,
    /// Who did it, and to whom (the same for events about one fighter).
    pub from: NetId,
    pub to: NetId,
    /// Where it happened (the point hit).
    pub at: [f32; 3],
    /// Damage dealt.
    pub amount: u16,
    /// The blow knocked the target back.
    pub knock: bool,
    /// The blow defeated the target.
    pub defeat: bool,
    /// The theme of the power that did it (0 or 1), for its look.
    pub theme: u8,
}

/// Bytes an [`Fx`] takes after its message type.
const FX_BYTES: usize = 8 + 1 + 4 + 4 + 12 + 2 + 1 + 1;

/// Reliable news from the server: repeated until the client confirms it,
/// delivered in order, exactly once.
#[derive(Clone, Debug, PartialEq)]
pub enum ToClient {
    Joined {
        player: PlayerId,
        name: String,
    },
    Left {
        player: PlayerId,
        name: String,
    },
    Chat {
        from: PlayerId,
        text: String,
    },
    /// The host is closing the world; the client should leave.
    SessionEnd {
        reason: String,
    },
    /// Elements of a building broke (indices into its breakable plan, see
    /// `hw_sim::destruction`). `live`: it happened just now, so play it;
    /// otherwise it is old damage a joining player catches up on.
    Damage {
        building: u64,
        broken: Vec<u32>,
        live: bool,
    },
    /// A building was repaired: it stands whole again.
    Repaired {
        building: u64,
    },
    /// Something was hit (for sparks, dust, marks and sound): where, the
    /// way the blow went, what hit it and whether anything broke.
    Impact {
        at: [f32; 3],
        dir: [f32; 3],
        kind: ImpactKind,
        broke: bool,
    },
    /// Street props broke: (the prop's id in the town, which way it fell
    /// as a yaw byte). `live` as for [`ToClient::Damage`].
    PropsBroken {
        props: Vec<(u64, u8)>,
        live: bool,
    },
    /// Props put back.
    PropsRepaired {
        props: Vec<u64>,
    },
    /// Moving cars wrecked: (vehicle id, the tick it happened, which way
    /// the blow went as a yaw byte, where along its lane it stands).
    /// `live` as for [`ToClient::Damage`].
    CarsWrecked {
        cars: Vec<(u64, u64, u8, f32)>,
        live: bool,
    },
    /// Street cars of one lane as they are, for a player who joins: the
    /// traffic time, the lane, the first car's number, and each car's
    /// (distance along, speed, steps waited or `u32::MAX` while off the
    /// road). Exact, so both machines drive on alike.
    Traffic {
        time: f64,
        lane: u32,
        first: u16,
        cars: Vec<(f64, f64, u32)>,
    },
    /// Wrecked cars towed away: their lanes drive on.
    CarsCleared {
        cars: Vec<u64>,
    },
    /// An incident somewhere in town (for the wrist radio): its number,
    /// kind (see `hw_sim::incidents::IncidentKind`), where, the villains'
    /// level, its state (0 open, 1 resolved, 2 timed out: gone), which way
    /// it faces (a yaw byte) and how many people are still trapped (a
    /// rescue; sent again each time one is got out).
    Incident {
        id: u32,
        kind: u8,
        at: [f32; 3],
        level: u8,
        state: u8,
        yaw: u8,
        left: u8,
    },
    /// The hurt person of a rescue (incident `id`): where the medic waits
    /// (and which way the ambulance faces, a yaw byte), and where the
    /// person is: lying at `at` (state 0), carried by hero `carrier`
    /// (state 1), or with the medic (state 2: gone).
    Casualty {
        id: u32,
        medic: [f32; 3],
        yaw: u8,
        state: u8,
        carrier: u32,
        at: [f32; 3],
    },
    /// Something happened in a fight nearby.
    Fx(Fx),
    /// Your hero's progress changed (karma, level, powers): keep it in
    /// your hero file.
    Sheet(Sheet),
    /// A piece of a player's looks (see [`Look`]): `part` of `parts`.
    Look {
        player: PlayerId,
        part: u8,
        parts: u8,
        bytes: Vec<u8>,
    },
    /// Trouble bystanders notice (see `hw_sim::alarms`): its number, kind,
    /// where, how far it is noticed; raised or moved (`live`), or over.
    Alarm {
        id: u32,
        kind: u8,
        at: [f32; 3],
        radius: f32,
        live: bool,
    },
    /// The square with people and traffic moved to follow the heroes:
    /// its centre (x, z). Traffic news after it are for the new square.
    AreaMoved {
        x: f64,
        z: f64,
    },
}

/// Most broken elements in one [`ToClient::Damage`] (it must fit a packet
/// next to everything else).
pub const MAX_DAMAGE_IDS: usize = 24;

/// What hit something.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImpactKind {
    Punch,
    Blast,
}

/// Reliable messages from a client.
#[derive(Clone, Debug, PartialEq)]
pub enum ToServer {
    Chat {
        text: String,
    },
    /// Hail a taxi to a stand (its index), or 255: get out.
    Taxi {
        dest: u8,
    },
    /// Ride the Aether Tube to a station (its index), or 65535: get out.
    Tube {
        dest: u16,
    },
    /// The hero file this player plays with (sent once on joining).
    Sheet(Sheet),
    /// Buy a power, in a theme.
    Buy {
        power: Power,
        theme: u8,
    },
    /// Give a power up (90% of its price comes back).
    GiveUp {
        power: Power,
    },
    /// A piece of this player's looks (see [`Look`]), sent on joining.
    Look {
        part: u8,
        parts: u8,
        bytes: Vec<u8>,
    },
}

impl ToClient {
    pub fn wire_size(&self) -> usize {
        let s = |t: &str| 1 + t.len().min(255);
        1 + match self {
            ToClient::Joined { name, .. } | ToClient::Left { name, .. } => 2 + s(name),
            ToClient::Chat { text, .. } => 2 + s(text),
            ToClient::SessionEnd { reason } => s(reason),
            ToClient::Damage { broken, .. } => 8 + 1 + 1 + 4 * broken.len().min(255),
            ToClient::Repaired { .. } => 8,
            ToClient::Impact { .. } => 12 + 6 + 1 + 1,
            ToClient::PropsBroken { props, .. } => 1 + 1 + 9 * props.len().min(255),
            ToClient::PropsRepaired { props } => 1 + 8 * props.len().min(255),
            ToClient::CarsWrecked { cars, .. } => 1 + 1 + 21 * cars.len().min(255),
            ToClient::Traffic { cars, .. } => 8 + 4 + 2 + 1 + 20 * cars.len().min(255),
            ToClient::CarsCleared { cars } => 1 + 8 * cars.len().min(255),
            ToClient::Incident { .. } => 4 + 1 + 12 + 1 + 1 + 1 + 1,
            ToClient::Casualty { .. } => 4 + 12 + 1 + 1 + 4 + 12,
            ToClient::Fx(_) => FX_BYTES,
            ToClient::Sheet(s) => s.wire_size(),
            ToClient::Look { bytes, .. } => 2 + 2 + 2 + bytes.len(),
            ToClient::Alarm { .. } => 4 + 1 + 12 + 4 + 1,
            ToClient::AreaMoved { .. } => 16,
        }
    }

    fn write(&self, w: &mut Writer) {
        match self {
            ToClient::Joined { player, name } => {
                w.u8(1).u16(player.0).str(name);
            }
            ToClient::Left { player, name } => {
                w.u8(2).u16(player.0).str(name);
            }
            ToClient::Chat { from, text } => {
                w.u8(3).u16(from.0).str(text);
            }
            ToClient::SessionEnd { reason } => {
                w.u8(4).str(reason);
            }
            ToClient::Damage {
                building,
                broken,
                live,
            } => {
                let n = broken.len().min(255);
                w.u8(5).u64(*building).bool(*live).u8(n as u8);
                for id in &broken[..n] {
                    w.u32(*id);
                }
            }
            ToClient::Repaired { building } => {
                w.u8(6).u64(*building);
            }
            ToClient::Impact {
                at,
                dir,
                kind,
                broke,
            } => {
                w.u8(7).f32(at[0]).f32(at[1]).f32(at[2]);
                for d in dir {
                    w.u16(((d.clamp(-1.0, 1.0) * 32767.0).round() as i16) as u16);
                }
                w.u8(match kind {
                    ImpactKind::Punch => 0,
                    ImpactKind::Blast => 1,
                })
                .bool(*broke);
            }
            ToClient::PropsBroken { props, live } => {
                let n = props.len().min(255);
                w.u8(8).bool(*live).u8(n as u8);
                for (i, fell) in &props[..n] {
                    w.u64(*i).u8(*fell);
                }
            }
            ToClient::PropsRepaired { props } => {
                let n = props.len().min(255);
                w.u8(9).u8(n as u8);
                for i in &props[..n] {
                    w.u64(*i);
                }
            }
            ToClient::CarsWrecked { cars, live } => {
                let n = cars.len().min(255);
                w.u8(12).bool(*live).u8(n as u8);
                for (id, tick, fell, along) in &cars[..n] {
                    w.u64(*id).u64(*tick).u8(*fell).f32(*along);
                }
            }
            ToClient::Traffic {
                time,
                lane,
                first,
                cars,
            } => {
                let n = cars.len().min(255);
                w.u8(14).f64(*time).u32(*lane).u16(*first).u8(n as u8);
                for (s, v, wait) in &cars[..n] {
                    w.f64(*s).f64(*v).u32(*wait);
                }
            }
            ToClient::CarsCleared { cars } => {
                let n = cars.len().min(255);
                w.u8(13).u8(n as u8);
                for id in &cars[..n] {
                    w.u64(*id);
                }
            }
            ToClient::Incident {
                id,
                kind,
                at,
                level,
                state,
                yaw,
                left,
            } => {
                w.u8(15).u32(*id).u8(*kind);
                for v in at {
                    w.f32(*v);
                }
                w.u8(*level).u8(*state).u8(*yaw).u8(*left);
            }
            ToClient::Casualty {
                id,
                medic,
                yaw,
                state,
                carrier,
                at,
            } => {
                w.u8(16).u32(*id);
                for v in medic {
                    w.f32(*v);
                }
                w.u8(*yaw).u8(*state).u32(*carrier);
                for v in at {
                    w.f32(*v);
                }
            }
            ToClient::Fx(fx) => {
                w.u8(10)
                    .u64(fx.tick)
                    .u8(fx.kind.code())
                    .u32(fx.from.0)
                    .u32(fx.to.0);
                for v in fx.at {
                    w.f32(v);
                }
                w.u16(fx.amount)
                    .u8(fx.knock as u8 | (fx.defeat as u8) << 1)
                    .u8(fx.theme);
            }
            ToClient::Sheet(sheet) => {
                w.u8(11);
                sheet.write(w);
            }
            ToClient::Look {
                player,
                part,
                parts,
                bytes,
            } => {
                w.u8(17).u16(player.0).u8(*part).u8(*parts).bytes(bytes);
            }
            ToClient::Alarm {
                id,
                kind,
                at,
                radius,
                live,
            } => {
                w.u8(18).u32(*id).u8(*kind);
                for v in at {
                    w.f32(*v);
                }
                w.f32(*radius).bool(*live);
            }
            ToClient::AreaMoved { x, z } => {
                w.u8(19).f64(*x).f64(*z);
            }
        }
    }

    fn read(r: &mut Reader) -> Result<Self, WireError> {
        Ok(match r.u8()? {
            1 => ToClient::Joined {
                player: PlayerId(r.u16()?),
                name: r.str()?,
            },
            2 => ToClient::Left {
                player: PlayerId(r.u16()?),
                name: r.str()?,
            },
            3 => ToClient::Chat {
                from: PlayerId(r.u16()?),
                text: r.str()?,
            },
            4 => ToClient::SessionEnd { reason: r.str()? },
            5 => {
                let building = r.u64()?;
                let live = r.bool()?;
                let n = r.u8()? as usize;
                let mut broken = Vec::with_capacity(n);
                for _ in 0..n {
                    broken.push(r.u32()?);
                }
                ToClient::Damage {
                    building,
                    broken,
                    live,
                }
            }
            6 => ToClient::Repaired { building: r.u64()? },
            7 => {
                let at = [r.f32()?, r.f32()?, r.f32()?];
                if at.iter().any(|v| !v.is_finite()) {
                    return Err(WireError);
                }
                let mut dir = [0.0f32; 3];
                for d in dir.iter_mut() {
                    *d = r.u16()? as i16 as f32 / 32767.0;
                }
                let kind = match r.u8()? {
                    0 => ImpactKind::Punch,
                    1 => ImpactKind::Blast,
                    _ => return Err(WireError),
                };
                ToClient::Impact {
                    at,
                    dir,
                    kind,
                    broke: r.bool()?,
                }
            }
            8 => {
                let live = r.bool()?;
                let n = r.u8()? as usize;
                let mut props = Vec::with_capacity(n);
                for _ in 0..n {
                    props.push((r.u64()?, r.u8()?));
                }
                ToClient::PropsBroken { props, live }
            }
            9 => {
                let n = r.u8()? as usize;
                let mut props = Vec::with_capacity(n);
                for _ in 0..n {
                    props.push(r.u64()?);
                }
                ToClient::PropsRepaired { props }
            }
            12 => {
                let live = r.bool()?;
                let n = r.u8()? as usize;
                let mut cars = Vec::with_capacity(n);
                for _ in 0..n {
                    cars.push((r.u64()?, r.u64()?, r.u8()?, r.f32()?));
                }
                ToClient::CarsWrecked { cars, live }
            }
            14 => {
                let time = r.f64()?;
                let lane = r.u32()?;
                let first = r.u16()?;
                let n = r.u8()? as usize;
                let mut cars = Vec::with_capacity(n);
                for _ in 0..n {
                    cars.push((r.f64()?, r.f64()?, r.u32()?));
                }
                ToClient::Traffic {
                    time,
                    lane,
                    first,
                    cars,
                }
            }
            13 => {
                let n = r.u8()? as usize;
                let mut cars = Vec::with_capacity(n);
                for _ in 0..n {
                    cars.push(r.u64()?);
                }
                ToClient::CarsCleared { cars }
            }
            10 => {
                let tick = r.u64()?;
                let kind = FxKind::from_code(r.u8()?)?;
                let from = NetId(r.u32()?);
                let to = NetId(r.u32()?);
                let at = [r.f32()?, r.f32()?, r.f32()?];
                let amount = r.u16()?;
                let bits = r.u8()?;
                if bits >> 2 != 0 {
                    return Err(WireError);
                }
                ToClient::Fx(Fx {
                    tick,
                    kind,
                    from,
                    to,
                    at,
                    amount,
                    knock: bits & 1 != 0,
                    defeat: bits & 2 != 0,
                    theme: r.u8()?,
                })
            }
            11 => ToClient::Sheet(Sheet::read(r)?),
            16 => ToClient::Casualty {
                id: r.u32()?,
                medic: [r.f32()?, r.f32()?, r.f32()?],
                yaw: r.u8()?,
                state: r.u8()?,
                carrier: r.u32()?,
                at: [r.f32()?, r.f32()?, r.f32()?],
            },
            19 => ToClient::AreaMoved {
                x: r.f64()?,
                z: r.f64()?,
            },
            18 => ToClient::Alarm {
                id: r.u32()?,
                kind: r.u8()?,
                at: [r.f32()?, r.f32()?, r.f32()?],
                radius: r.f32()?,
                live: r.bool()?,
            },
            17 => ToClient::Look {
                player: PlayerId(r.u16()?),
                part: r.u8()?,
                parts: r.u8()?,
                bytes: r.bytes()?,
            },
            15 => ToClient::Incident {
                id: r.u32()?,
                kind: r.u8()?,
                at: [r.f32()?, r.f32()?, r.f32()?],
                level: r.u8()?,
                state: r.u8()?,
                yaw: r.u8()?,
                left: r.u8()?,
            },
            _ => return Err(WireError),
        })
    }
}

impl ToServer {
    pub fn wire_size(&self) -> usize {
        match self {
            ToServer::Chat { text } => 2 + text.len().min(255),
            ToServer::Taxi { .. } => 2,
            ToServer::Tube { .. } => 3,
            ToServer::Sheet(s) => 1 + s.wire_size(),
            ToServer::Buy { .. } => 3,
            ToServer::GiveUp { .. } => 2,
            ToServer::Look { bytes, .. } => 1 + 2 + 2 + bytes.len(),
        }
    }

    fn write(&self, w: &mut Writer) {
        match self {
            ToServer::Chat { text } => {
                w.u8(1).str(text);
            }
            ToServer::Taxi { dest } => {
                w.u8(2).u8(*dest);
            }
            ToServer::Sheet(sheet) => {
                w.u8(3);
                sheet.write(w);
            }
            ToServer::Buy { power, theme } => {
                w.u8(4).u8(power.code()).u8(*theme);
            }
            ToServer::GiveUp { power } => {
                w.u8(5).u8(power.code());
            }
            ToServer::Look { part, parts, bytes } => {
                w.u8(6).u8(*part).u8(*parts).bytes(bytes);
            }
            ToServer::Tube { dest } => {
                w.u8(7).u16(*dest);
            }
        }
    }

    fn read(r: &mut Reader) -> Result<Self, WireError> {
        Ok(match r.u8()? {
            1 => ToServer::Chat { text: r.str()? },
            2 => ToServer::Taxi { dest: r.u8()? },
            3 => ToServer::Sheet(Sheet::read(r)?),
            4 => ToServer::Buy {
                power: Power::from_code(r.u8()?).ok_or(WireError)?,
                theme: r.u8()?,
            },
            6 => ToServer::Look {
                part: r.u8()?,
                parts: r.u8()?,
                bytes: r.bytes()?,
            },
            5 => ToServer::GiveUp {
                power: Power::from_code(r.u8()?).ok_or(WireError)?,
            },
            7 => ToServer::Tube { dest: r.u16()? },
            _ => return Err(WireError),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ClientMsg {
    Hello {
        name: String,
    },
    Input {
        /// Sequence number of the newest input (the last in `inputs`).
        seq: u32,
        /// The newest inputs, oldest first, at most [`INPUT_REDUNDANCY`].
        inputs: Vec<InputState>,
        /// The newest update tick received, and which of the 32 ticks before
        /// it arrived too (bit 0 = `ack_tick - 1`).
        ack_tick: u64,
        ack_bits: u32,
        /// Every reliable message from the server below this ID has arrived.
        rel_ack: u32,
        /// Reliable messages not yet confirmed by the server.
        reliable: Vec<(u32, ToServer)>,
    },
    Bye,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ServerMsg {
    Welcome {
        player: PlayerId,
        hero: NetId,
        world_seed: u64,
        /// People in the town: with the seed, it makes the world.
        town_population: f64,
        tick_hz: u16,
    },
    Reject {
        reason: String,
    },
    Update {
        tick: u64,
        /// Last input sequence the server applied for this player.
        ack: u32,
        /// Every reliable message from this client below this ID has arrived.
        rel_ack: u32,
        reliable: Vec<(u32, ToClient)>,
        /// Entities that changed (whole state each, so any update can be
        /// applied on its own).
        entities: Vec<EntityState>,
        /// Entities the client should forget (gone, or out of its range).
        removed: Vec<NetId>,
    },
}

/// Bytes an update takes before its entities, reliable messages and removals.
pub const UPDATE_HEADER_BYTES: usize = 4 + 4 + 1 + 8 + 4 + 4 + 1 + 2 + 2;
/// Bytes per removed entity.
pub const REMOVED_BYTES: usize = 4;

fn header(w: &mut Writer, kind: u8) {
    w.buf.extend_from_slice(&MAGIC);
    w.u32(PROTOCOL_VERSION).u8(kind);
}

/// Checks magic and version; returns the message type.
fn read_header(r: &mut Reader) -> Result<u8, WireError> {
    let magic = [r.u8()?, r.u8()?, r.u8()?, r.u8()?];
    if magic != MAGIC || r.u32()? != PROTOCOL_VERSION {
        return Err(WireError);
    }
    r.u8()
}

fn write_input(w: &mut Writer, input: &InputState) {
    w.f32(input.move_x)
        .f32(input.move_z)
        .f32(input.move_y)
        .u8(input.jump as u8
            | (input.descend as u8) << 1
            | (input.fly as u8) << 2
            | (input.boost as u8) << 3
            | (input.smash as u8) << 4
            | (input.blast as u8) << 5
            | (input.revive as u8) << 6
            | (input.hospital as u8) << 7);
    for a in input.aim {
        w.u16(((a.clamp(-1.0, 1.0) * 32767.0).round() as i16) as u16);
    }
    w.u32(input.target.map_or(0, |t| t.0)).u8(input.power);
}

fn read_input(r: &mut Reader) -> Result<InputState, WireError> {
    let (x, z, y) = (r.f32()?, r.f32()?, r.f32()?);
    let len = (x * x + y * y + z * z).sqrt();
    let k = if len > 1.0 { 1.0 / len } else { 1.0 };
    let bits = r.u8()?;
    let mut aim = [0.0f32; 3];
    for a in aim.iter_mut() {
        *a = r.u16()? as i16 as f32 / 32767.0;
    }
    let target = match r.u32()? {
        0 => None,
        t => Some(NetId(t)),
    };
    let power = r.u8()?;
    if power != 0 && Power::from_code(power).is_none() {
        return Err(WireError);
    }
    Ok(InputState {
        move_x: x * k,
        move_y: y * k,
        move_z: z * k,
        jump: bits & 1 != 0,
        descend: bits & 2 != 0,
        fly: bits & 4 != 0,
        boost: bits & 8 != 0,
        smash: bits & 16 != 0,
        blast: bits & 32 != 0,
        aim,
        revive: bits & 64 != 0,
        hospital: bits & 128 != 0,
        target,
        power,
    })
}

impl ClientMsg {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        match self {
            ClientMsg::Hello { name } => {
                header(&mut w, 1);
                w.str(name);
            }
            ClientMsg::Input {
                seq,
                inputs,
                ack_tick,
                ack_bits,
                rel_ack,
                reliable,
            } => {
                header(&mut w, 2);
                let inputs = &inputs[inputs.len().saturating_sub(INPUT_REDUNDANCY)..];
                w.u32(*seq).u8(inputs.len() as u8);
                for i in inputs {
                    write_input(&mut w, i);
                }
                w.u64(*ack_tick).u32(*ack_bits).u32(*rel_ack);
                // Reliable messages that would overflow the packet wait.
                let mut fit = Vec::new();
                let mut room = MAX_PACKET.saturating_sub(w.buf.len() + 1);
                for (id, m) in reliable.iter().take(255) {
                    let n = 4 + m.wire_size();
                    if n > room {
                        break;
                    }
                    room -= n;
                    fit.push((id, m));
                }
                w.u8(fit.len() as u8);
                for (id, m) in fit {
                    w.u32(*id);
                    m.write(&mut w);
                }
            }
            ClientMsg::Bye => header(&mut w, 3),
        }
        w.buf
    }

    pub fn decode(buf: &[u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(buf);
        let msg = match read_header(&mut r)? {
            1 => ClientMsg::Hello { name: r.str()? },
            2 => {
                let seq = r.u32()?;
                let n = r.u8()? as usize;
                if n == 0 || n > INPUT_REDUNDANCY {
                    return Err(WireError);
                }
                let mut inputs = Vec::with_capacity(n);
                for _ in 0..n {
                    inputs.push(read_input(&mut r)?);
                }
                let ack_tick = r.u64()?;
                let ack_bits = r.u32()?;
                let rel_ack = r.u32()?;
                let k = r.u8()? as usize;
                let mut reliable = Vec::with_capacity(k);
                for _ in 0..k {
                    let id = r.u32()?;
                    reliable.push((id, ToServer::read(&mut r)?));
                }
                ClientMsg::Input {
                    seq,
                    inputs,
                    ack_tick,
                    ack_bits,
                    rel_ack,
                    reliable,
                }
            }
            3 => ClientMsg::Bye,
            _ => return Err(WireError),
        };
        r.is_done().then_some(msg).ok_or(WireError)
    }
}

impl ServerMsg {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        match self {
            ServerMsg::Welcome {
                player,
                hero,
                world_seed,
                town_population,
                tick_hz,
            } => {
                header(&mut w, 1);
                w.u16(player.0)
                    .u32(hero.0)
                    .u64(*world_seed)
                    .f64(*town_population)
                    .u16(*tick_hz);
            }
            ServerMsg::Reject { reason } => {
                header(&mut w, 2);
                w.str(reason);
            }
            ServerMsg::Update {
                tick,
                ack,
                rel_ack,
                reliable,
                entities,
                removed,
            } => {
                // The sender sizes updates with `wire_size`; anything past
                // the packet limit is cut here as a last resort.
                header(&mut w, 3);
                w.u64(*tick).u32(*ack).u32(*rel_ack);
                let mut room = MAX_PACKET - UPDATE_HEADER_BYTES;
                let mut rel = Vec::new();
                for (id, m) in reliable.iter().take(255) {
                    let n = 4 + m.wire_size();
                    if n > room {
                        break;
                    }
                    room -= n;
                    rel.push((id, m));
                }
                w.u8(rel.len() as u8);
                for (id, m) in rel {
                    w.u32(*id);
                    m.write(&mut w);
                }
                let gone = removed
                    .len()
                    .min(room / REMOVED_BYTES)
                    .min(u16::MAX as usize);
                room -= gone * REMOVED_BYTES;
                w.u16(gone as u16);
                for id in &removed[..gone] {
                    w.u32(id.0);
                }
                let mut n = 0;
                for e in entities.iter().take(u16::MAX as usize) {
                    let s = e.wire_size();
                    if s > room {
                        break;
                    }
                    room -= s;
                    n += 1;
                }
                w.u16(n as u16);
                for e in &entities[..n] {
                    e.write(&mut w);
                }
            }
        }
        w.buf
    }

    pub fn decode(buf: &[u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(buf);
        let msg = match read_header(&mut r)? {
            1 => ServerMsg::Welcome {
                player: PlayerId(r.u16()?),
                hero: NetId(r.u32()?),
                world_seed: r.u64()?,
                town_population: r.f64()?,
                tick_hz: r.u16()?,
            },
            2 => ServerMsg::Reject { reason: r.str()? },
            3 => {
                let tick = r.u64()?;
                let ack = r.u32()?;
                let rel_ack = r.u32()?;
                let k = r.u8()? as usize;
                let mut reliable = Vec::with_capacity(k);
                for _ in 0..k {
                    let id = r.u32()?;
                    reliable.push((id, ToClient::read(&mut r)?));
                }
                let g = r.u16()? as usize;
                if g * REMOVED_BYTES > MAX_PACKET {
                    return Err(WireError);
                }
                let mut removed = Vec::with_capacity(g);
                for _ in 0..g {
                    removed.push(NetId(r.u32()?));
                }
                let n = r.u16()? as usize;
                if n > MAX_PACKET / 30 {
                    return Err(WireError);
                }
                let mut entities = Vec::with_capacity(n);
                for _ in 0..n {
                    entities.push(EntityState::read(&mut r)?);
                }
                ServerMsg::Update {
                    tick,
                    ack,
                    rel_ack,
                    reliable,
                    entities,
                    removed,
                }
            }
            _ => return Err(WireError),
        };
        r.is_done().then_some(msg).ok_or(WireError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hero(id: u32) -> EntityState {
        EntityState {
            id: NetId(id),
            pos: [1.0, 2.0, -3.5],
            vel: [0.5, 0.0, 0.0],
            grounded: true,
            owner: Some(PlayerId(1)),
            ..Default::default()
        }
    }

    fn sheet() -> Sheet {
        Sheet {
            lifetime: 1234,
            karma: 56,
            affinity: [1, 2, 3, 4, 500],
            origin: 3,
            powers: vec![
                OwnedPower {
                    power: Power::Bolt,
                    theme: 1,
                    paid: 40,
                },
                OwnedPower {
                    power: Power::Flight,
                    theme: 0,
                    paid: 300,
                },
            ],
            regard: [0, 5, -40, -100, 12, 127, -7, 60],
        }
    }

    #[test]
    fn client_messages_round_trip() {
        for m in [
            ClientMsg::Hello {
                name: "Captain".into(),
            },
            ClientMsg::Input {
                seq: 9,
                inputs: vec![
                    InputState::default(),
                    InputState {
                        move_x: 0.6,
                        move_z: -0.8,
                        move_y: 0.0,
                        jump: true,
                        descend: false,
                        fly: true,
                        boost: false,
                        smash: true,
                        blast: false,
                        aim: [0.0, -1.0, 0.0],
                        target: Some(NetId(17)),
                        revive: true,
                        hospital: false,
                        power: 4,
                    },
                ],
                ack_tick: 1234,
                ack_bits: 0xF0F0_0001,
                rel_ack: 3,
                reliable: vec![
                    (
                        3,
                        ToServer::Chat {
                            text: "hello".into(),
                        },
                    ),
                    (4, ToServer::Sheet(sheet())),
                    (
                        5,
                        ToServer::Buy {
                            power: Power::Shield,
                            theme: 1,
                        },
                    ),
                    (6, ToServer::GiveUp { power: Power::Hold }),
                    (
                        7,
                        ToServer::Look {
                            part: 1,
                            parts: 3,
                            bytes: b"{\"hair\":null}".to_vec(),
                        },
                    ),
                    (8, ToServer::Tube { dest: 513 }),
                ],
            },
            ClientMsg::Bye,
        ] {
            assert_eq!(ClientMsg::decode(&m.encode()).unwrap(), m);
        }
    }

    #[test]
    fn server_messages_round_trip() {
        let update = ServerMsg::Update {
            tick: 77,
            ack: 5,
            rel_ack: 2,
            reliable: vec![
                (
                    0,
                    ToClient::Joined {
                        player: PlayerId(2),
                        name: "Bob".into(),
                    },
                ),
                (
                    1,
                    ToClient::SessionEnd {
                        reason: "host left".into(),
                    },
                ),
                (
                    2,
                    ToClient::Damage {
                        building: 812,
                        broken: vec![0, 7, 4093],
                        live: true,
                    },
                ),
                (3, ToClient::Repaired { building: 5 }),
                (
                    5,
                    ToClient::PropsBroken {
                        props: vec![(3, 200), (9001, 0)],
                        live: false,
                    },
                ),
                (6, ToClient::PropsRepaired { props: vec![3] }),
                (
                    7,
                    ToClient::CarsWrecked {
                        cars: vec![(1 << 40 | 3 << 16 | 2, 9000, 77, 31.5)],
                        live: true,
                    },
                ),
                (
                    9,
                    ToClient::Traffic {
                        time: 12.3,
                        lane: 7,
                        first: 48,
                        cars: vec![(10.5, 9.25, 0), (40.0, 0.0, u32::MAX)],
                    },
                ),
                (8, ToClient::CarsCleared { cars: vec![65538] }),
                (
                    8,
                    ToClient::AreaMoved {
                        x: 1500.5,
                        z: -3000.0,
                    },
                ),
                (
                    9,
                    ToClient::Incident {
                        id: 7,
                        kind: 2,
                        at: [1.0, 2.0, -3.5],
                        level: 4,
                        state: 1,
                        yaw: 200,
                        left: 2,
                    },
                ),
                (
                    10,
                    ToClient::Casualty {
                        id: 7,
                        medic: [40.0, 2.0, -3.5],
                        yaw: 64,
                        state: 1,
                        carrier: 12,
                        at: [1.0, 2.5, -3.0],
                    },
                ),
                (
                    4,
                    ToClient::Impact {
                        at: [12.5, 3.25, -40.0],
                        dir: [0.0, 0.0, -1.0],
                        kind: ImpactKind::Blast,
                        broke: true,
                    },
                ),
                (
                    5,
                    ToClient::Fx(Fx {
                        tick: 76,
                        kind: FxKind::Bolt,
                        from: NetId(3),
                        to: NetId(12),
                        at: [1.0, 2.0, 3.5],
                        amount: 36,
                        knock: true,
                        defeat: false,
                        theme: 1,
                    }),
                ),
            ],
            entities: vec![
                hero(3),
                EntityState {
                    id: NetId(4),
                    kind: EntityKind::Crate,
                    pos: [100.25, 0.0, 9.0],
                    rot: [0.0, 0.6, 0.0, 0.8],
                    ..Default::default()
                },
                EntityState {
                    id: NetId(12),
                    kind: EntityKind::Brute,
                    combat: Some(CombatState {
                        health: 128,
                        energy: 255,
                        status: Status::Knocked,
                        level: 1,
                        target: Some(NetId(3)),
                        effects: EFFECT_HELD | EFFECT_SHIELD,
                    }),
                    ..Default::default()
                },
            ],
            removed: vec![NetId(9), NetId(11)],
        };
        for m in [
            ServerMsg::Welcome {
                player: PlayerId(2),
                hero: NetId(8),
                world_seed: 42,
                town_population: 1234.5,
                tick_hz: 30,
            },
            ServerMsg::Reject {
                reason: "full".into(),
            },
            update,
        ] {
            let bytes = m.encode();
            assert!(bytes.len() <= MAX_PACKET);
            assert_eq!(ServerMsg::decode(&bytes).unwrap(), m);
        }
    }

    #[test]
    fn rejects_foreign_and_overlong_input() {
        assert!(ClientMsg::decode(b"nonsense").is_err());
        let mut bytes = ClientMsg::Bye.encode();
        bytes.push(0);
        assert!(ClientMsg::decode(&bytes).is_err());
        // Over-long movement is clamped to length 1.
        let m = ClientMsg::Input {
            seq: 1,
            inputs: vec![InputState {
                move_x: 3.0,
                move_z: 4.0,
                move_y: 0.0,
                jump: false,
                descend: true,
                fly: false,
                boost: true,
                ..Default::default()
            }],
            ack_tick: 0,
            ack_bits: 0,
            rel_ack: 0,
            reliable: Vec::new(),
        };
        let ClientMsg::Input { inputs, .. } = ClientMsg::decode(&m.encode()).unwrap() else {
            panic!()
        };
        let i = inputs[0];
        assert!(((i.move_x.powi(2) + i.move_z.powi(2)).sqrt() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn sizes_are_what_the_encoder_writes() {
        let crate_box = EntityState {
            kind: EntityKind::Crate,
            rot: [0.0, 0.0, 0.6, 0.8],
            ..hero(2)
        };
        let brute = EntityState {
            kind: EntityKind::Brute,
            combat: Some(CombatState::default()),
            ..hero(5)
        };
        for e in [hero(1), crate_box, brute, EntityState::default()] {
            let one = ServerMsg::Update {
                tick: 0,
                ack: 0,
                rel_ack: 0,
                reliable: Vec::new(),
                entities: vec![e],
                removed: Vec::new(),
            };
            assert_eq!(one.encode().len(), UPDATE_HEADER_BYTES + e.wire_size());
        }
        let m = ToClient::Chat {
            from: PlayerId(1),
            text: "hi there".into(),
        };
        let one = ServerMsg::Update {
            tick: 0,
            ack: 0,
            rel_ack: 0,
            reliable: vec![(7, m.clone())],
            entities: Vec::new(),
            removed: Vec::new(),
        };
        assert_eq!(one.encode().len(), UPDATE_HEADER_BYTES + 4 + m.wire_size());
        let m = ToClient::Sheet(sheet());
        let one = ServerMsg::Update {
            tick: 0,
            ack: 0,
            rel_ack: 0,
            reliable: vec![(7, m.clone())],
            entities: Vec::new(),
            removed: Vec::new(),
        };
        assert_eq!(one.encode().len(), UPDATE_HEADER_BYTES + 4 + m.wire_size());
        assert_eq!(ServerMsg::decode(&one.encode()).unwrap(), one);
        let m = ToClient::Look {
            player: PlayerId(3),
            part: 0,
            parts: 2,
            bytes: vec![b'x'; crate::look::LOOK_PIECE],
        };
        let one = ServerMsg::Update {
            tick: 0,
            ack: 0,
            rel_ack: 0,
            reliable: vec![(8, m.clone())],
            entities: Vec::new(),
            removed: Vec::new(),
        };
        assert_eq!(one.encode().len(), UPDATE_HEADER_BYTES + 4 + m.wire_size());
        assert_eq!(ServerMsg::decode(&one.encode()).unwrap(), one);
    }

    #[test]
    fn an_overfull_update_is_cut_to_a_packet() {
        let bytes = ServerMsg::Update {
            tick: 0,
            ack: 0,
            rel_ack: 0,
            reliable: Vec::new(),
            entities: vec![hero(1); 100],
            removed: vec![NetId(1); 50],
        }
        .encode();
        assert!(bytes.len() <= MAX_PACKET, "{} bytes", bytes.len());
        assert!(ServerMsg::decode(&bytes).is_ok());
    }
}
