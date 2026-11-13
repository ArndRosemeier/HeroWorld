//! Every power as one record (the power redesign, 2026-11-10).
//!
//! A power is a [`PowerDef`]: where it sits in a tree, what it needs, the
//! mechanic it is built on ([`Power`]: how it is delivered and how it
//! looks) and its [`Fields`]: numbers for what it does to others, to the
//! hero and their friends, and to how the hero moves. Most fields are zero
//! for most powers; a special is just a field that is usually zero, so it
//! can later ride as a small extra on another power (augments, power
//! customisation). Energy and recharge come from the fields
//! ([`energy`], [`recharge`]), not from a table.
//!
//! Every hero owns Strike and picks powers from their origin's tree
//! ([`Tree::of_origin`]) and the common tree (travel). A power needs the
//! ones its arrows come from ([`PowerDef::needs`]: each inner list is "one
//! of these"); powers in one fork group exclude each other for good.
//! Prices go by row ([`crate::progress::price`]).
//!
//! Each hero also has one [`Element`], picked at creation: it rides on
//! every blow, scaled by the blow's damage.

use hw_net::{Augment, Power, PowerId, Sheet, POWER_IDS, SOCKETS};

/// The trees powers sit in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Tree {
    /// Strike: everyone's.
    Basic,
    /// Travel powers, open to every origin.
    Common,
    Science,
    Mystic,
    Might,
    Guardian,
    Justice,
}

impl Tree {
    /// The trees a hero of each origin uses (by `progress::ORIGINS`).
    pub const ORIGIN_TREES: [Tree; 5] = [
        Tree::Science,
        Tree::Mystic,
        Tree::Might,
        Tree::Guardian,
        Tree::Justice,
    ];

    pub fn of_origin(origin: u8) -> Tree {
        Self::ORIGIN_TREES[(origin as usize).min(4)]
    }

    pub fn name(self) -> &'static str {
        match self {
            Tree::Basic => "Basic",
            Tree::Common => "Travel",
            Tree::Science => "Lab Accident",
            Tree::Mystic => "Ancient Relic",
            Tree::Might => "Born Mighty",
            Tree::Guardian => "Sworn Guardian",
            Tree::Justice => "Masked Avenger",
        }
    }
}

/// Where a moving power ends up, and where its blast goes off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lands {
    /// At the spot aimed at (a teleport); a blast there too.
    Target,
    /// A blast where the hero set off.
    Start,
    /// A blast where the hero arrives.
    Arrival,
    /// Everyone along the way is hit (a dash).
    Path,
    /// Home to Hero Hall.
    Home,
    /// The portal's first button: marks where its portals lead (see
    /// [`crate::gates`]); the hero stays.
    GateMark,
    /// The portal's second button: opens a portal to the mark.
    GateOpen,
}

/// Travel bits ([`Fields::travel`]).
pub mod travel {
    pub const JUMP: u8 = 1;
    pub const SPRINT: u8 = 2;
    pub const GLIDE: u8 = 4;
    pub const FLY: u8 = 8;
    pub const SPEED: u8 = 16;
    pub const WALL: u8 = 32;
}

/// What a power does: three groups of numbers (on others, on the hero and
/// friends, on movement). Zero means "not this".
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fields {
    // ---- Reach ----
    /// Metres to the target (0: the hero itself).
    pub range: f64,
    /// An area's radius (a burst, a strike from the sky, friends round
    /// the hero); a beam's half width; a cone's widening per metre.
    pub radius: f64,
    // ---- On others ----
    /// Damage at level 1 (grows with the level).
    pub damage: f32,
    /// How hard a hit staggers: at 1 every hit knocks down (others only
    /// when they defeat). Less builds up over several hits, sooner on
    /// minions than on bosses (see [`crate::combat::STAGGER_FLOORS`]).
    pub knock: f32,
    /// Throws the one hit up (m/s).
    pub launch: f32,
    /// Shoves the one hit away (m); below zero pulls toward the centre.
    pub push: f32,
    /// Seconds bound in place.
    pub hold: f64,
    /// Seconds frozen solid (minions; tougher ones are slowed instead).
    pub freeze: f64,
    /// Share slower on their feet and in their swings, for `slow_secs`.
    pub slow: f32,
    pub slow_secs: f64,
    /// Share of the hit that burns on over [`DOT_SECONDS`].
    pub dot: f32,
    /// Seconds wandering confused (attacks nobody).
    pub confuse: f64,
    /// Seconds running away.
    pub fear: f64,
    /// Seconds fighting for the hero.
    pub charm: f64,
    /// Seconds unable to attack.
    pub disarm: f64,
    /// The one hit takes this share more from everyone, for `lasts`.
    pub weaken: f32,
    /// This share of the hit leaps on to `spread_to` more villains near.
    pub spread: f32,
    pub spread_to: u8,
    /// This share of the damage comes back to the hero as health.
    pub drain: f32,
    /// This share more on a villain who is held, frozen, confused or afraid.
    pub exploit: f32,
    /// Blows in a row (a flurry), or strikes from the sky (one a second).
    pub repeats: u8,
    /// Seconds before a strike from the sky lands.
    pub delay: f64,
    /// How hard it hits the street round it (buildings, cars).
    pub wreck: f32,
    /// The share of the street and the bystanders round it that it
    /// spares (1: all of it).
    pub spare: f32,
    // ---- The hero and friends ----
    /// Health given back (at level 1).
    pub heal: f32,
    /// Gets downed friends in the area back on their feet, with this
    /// share of the health a revive gives (1: as a teammate's hand does).
    pub revive: f32,
    /// Share of every blow a shield stops, for `lasts`.
    pub shield: f32,
    /// Share of a blow sent back at the one who struck.
    pub reflect: f32,
    /// Share less damage taken (below zero: more).
    pub resist: f32,
    /// Share of health back every second.
    pub regen: f32,
    /// Share harder hits.
    pub might: f32,
    /// Share faster recharge.
    pub haste: f32,
    /// The share less time a knock keeps the hero down (1: never
    /// knocked down).
    pub steadfast: f32,
    /// Villains lose sight of the hero (until the hero strikes): they
    /// notice from this share less far (1: not at all).
    pub hide: f32,
    /// Minions left at or below this share of their health by a blow
    /// fall to it (1: at any single blow).
    pub execute: f32,
    /// Once in a while a blow that would down the hero leaves them
    /// standing; this many times as often (1: every
    /// [`crate::combat::ANGEL_EVERY`]).
    pub angel: f32,
    /// Friends near the hero (within [`AURA_RADIUS`]) hit this share harder.
    pub aura: f32,
    /// Friends near the hero take this share less.
    pub ward: f32,
    /// Seconds a shield, a buff or a weakening lasts.
    pub lasts: f64,
    /// Sidekicks called, and how strong each is.
    pub summons: u8,
    pub summon_power: f32,
    /// People a bubble floats out at once.
    pub rescues: u8,
    /// Metres a downed friend can be revived from.
    pub revive_reach: f64,
    /// Share more karma for villains beaten.
    pub bounty: f32,
    /// Share of a punch that leaps on to one more villain near.
    pub chain: f32,
    /// The first blow on a villain who is not fighting the hero lands
    /// this share harder.
    pub ambush: f32,
    /// Share of villains' blows that miss the hero.
    pub dodge: f32,
    /// Share less taken while the hero stands still.
    pub brace: f32,
    /// A minion left at or below this share of its health gives up.
    pub cuff: f32,
    /// Once in a while, dropping below a quarter of health gives this
    /// share of it back.
    pub second_wind: f32,
    /// Seconds after going down that the hero gets up on their own (once
    /// in a while).
    pub get_up: f64,
    /// A toggle's share of the hero's energy recovery while it is on.
    pub upkeep: f32,
    // ---- Movement ----
    /// [`travel`] bits.
    pub travel: u8,
    /// Where a moving power ends up (teleports, dashes, leaps).
    pub lands: Lands,
    /// Damage a second to villains near a hero going fast.
    pub trail: f32,
    // ---- Only from augments (see [`crate::augments`]) ----
    /// Recharges this share of half its wait sooner (1: in half the time).
    pub quick: f32,
    /// Takes this share of half its energy less (1: half the energy).
    pub thrifty: f32,
    /// Share faster on the ground (Sprint, Super Speed), in the air
    /// (Flight, Glide), and the share higher a super jump goes.
    pub swift: f32,
    pub soar: f32,
    /// Share faster through the air on a super jump.
    pub leap: f32,
    pub height: f32,
    /// Share harder a super jump's landing stomp hits.
    pub stomp: f32,
}

impl Fields {
    pub const NONE: Fields = Fields {
        range: 0.0,
        radius: 0.0,
        damage: 0.0,
        knock: 0.0,
        launch: 0.0,
        push: 0.0,
        hold: 0.0,
        freeze: 0.0,
        slow: 0.0,
        slow_secs: 0.0,
        dot: 0.0,
        confuse: 0.0,
        fear: 0.0,
        charm: 0.0,
        disarm: 0.0,
        weaken: 0.0,
        spread: 0.0,
        spread_to: 0,
        drain: 0.0,
        exploit: 0.0,
        repeats: 0,
        delay: 0.0,
        wreck: 0.0,
        spare: 0.0,
        heal: 0.0,
        revive: 0.0,
        shield: 0.0,
        reflect: 0.0,
        resist: 0.0,
        regen: 0.0,
        might: 0.0,
        haste: 0.0,
        steadfast: 0.0,
        hide: 0.0,
        execute: 0.0,
        angel: 0.0,
        aura: 0.0,
        ward: 0.0,
        lasts: 0.0,
        summons: 0,
        summon_power: 0.0,
        rescues: 0,
        revive_reach: 0.0,
        bounty: 0.0,
        chain: 0.0,
        ambush: 0.0,
        dodge: 0.0,
        brace: 0.0,
        cuff: 0.0,
        second_wind: 0.0,
        get_up: 0.0,
        upkeep: 0.0,
        travel: 0,
        lands: Lands::Target,
        trail: 0.0,
        quick: 0.0,
        thrifty: 0.0,
        swift: 0.0,
        soar: 0.0,
        leap: 0.0,
        height: 0.0,
        stomp: 0.0,
    };

    /// Adds a passive power's fields to a sum of them (resistances
    /// multiply, so two can never reach immunity).
    fn add_passive(&mut self, o: &Fields) {
        self.resist = 1.0 - (1.0 - self.resist) * (1.0 - o.resist);
        self.regen += o.regen;
        self.might += o.might;
        self.haste += o.haste;
        self.steadfast = self.steadfast.max(o.steadfast);
        self.angel = self.angel.max(o.angel);
        self.aura = self.aura.max(o.aura);
        self.ward = self.ward.max(o.ward);
        self.reflect += o.reflect;
        self.exploit += o.exploit;
        self.ambush += o.ambush;
        self.dodge = 1.0 - (1.0 - self.dodge) * (1.0 - o.dodge);
        self.brace = 1.0 - (1.0 - self.brace) * (1.0 - o.brace);
        self.cuff = self.cuff.max(o.cuff);
        self.second_wind = self.second_wind.max(o.second_wind);
        self.get_up = if self.get_up > 0.0 && o.get_up > 0.0 {
            self.get_up.min(o.get_up)
        } else {
            self.get_up.max(o.get_up)
        };
        self.upkeep += o.upkeep;
        self.revive_reach = self.revive_reach.max(o.revive_reach);
        self.bounty += o.bounty;
        self.chain = self.chain.max(o.chain);
        self.travel |= o.travel;
        self.trail += o.trail;
        self.swift += o.swift;
        self.soar += o.soar;
        self.leap += o.leap;
        self.height += o.height;
        self.stomp += o.stomp;
    }
}

/// Seconds a burn ([`Fields::dot`]) lasts.
pub const DOT_SECONDS: f64 = 3.0;
/// Reach of auras ([`Fields::aura`], [`Fields::ward`]) (m).
pub const AURA_RADIUS: f64 = 20.0;

/// How a power is delivered: follows from its mechanic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    /// A blow at a villain in reach.
    Melee,
    /// A shot at one villain.
    Ranged,
    /// Everything along a line.
    Line,
    /// Everything in a widening cone.
    Cone,
    /// Everything round the hero.
    Burst,
    /// An area at the target, a moment later.
    Sky,
    /// The hero and friends round them.
    Allies,
    /// Lift something and throw it.
    Grab,
    /// The hero moves (teleport, dash, leap, home).
    Blink,
    /// A rescue bubble.
    Rescue,
    /// A sidekick.
    Summon,
    /// Always on.
    Passive,
}

/// How each mechanic is delivered.
pub fn delivery(base: Power) -> Delivery {
    match base {
        Power::Strike => Delivery::Melee,
        Power::Bolt | Power::Hold | Power::Hypno => Delivery::Ranged,
        Power::Beam => Delivery::Line,
        Power::Freeze => Delivery::Cone,
        Power::Shockwave | Power::Nova => Delivery::Burst,
        Power::Meteor => Delivery::Sky,
        Power::Shield | Power::Mend => Delivery::Allies,
        Power::Lift => Delivery::Grab,
        Power::Teleport => Delivery::Blink,
        Power::Bubble => Delivery::Rescue,
        Power::Sidekick => Delivery::Summon,
        Power::SuperJump
        | Power::Flight
        | Power::SuperSpeed
        | Power::Vitality
        | Power::Invulnerable
        | Power::Rally => Delivery::Passive,
    }
}

/// One power.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PowerDef {
    pub id: PowerId,
    pub name: &'static str,
    /// A few words on what it does.
    pub blurb: &'static str,
    pub tree: Tree,
    /// Row in its tree (0 at the top: the price goes by it) and column.
    pub row: u8,
    pub col: u8,
    /// Each inner list: one of these must be owned. All lists must be met.
    pub needs: &'static [&'static [u8]],
    /// Needs this many other powers of its own tree (any).
    pub needs_any: u8,
    /// Powers with the same nonzero fork are exclusive: pick one for good.
    pub fork: u8,
    /// A helping power (rescues, revives): drawn green.
    pub helper: bool,
    /// A capstone: the bottom of a tree.
    pub capstone: bool,
    /// Switched on and off with its key; while on it takes
    /// [`Fields::upkeep`] of the hero's energy recovery.
    pub toggle: bool,
    /// The mechanic it is built on (delivery and look).
    pub base: Power,
    /// What it is made of, in a word (fire, roots, void, ...).
    pub noun: &'static str,
    pub fields: Fields,
}

impl PowerDef {
    pub fn delivery(&self) -> Delivery {
        delivery(self.base)
    }

    /// Always on once owned (no key).
    pub fn passive(&self) -> bool {
        self.delivery() == Delivery::Passive && !self.toggle
    }

    /// Switched on and off with its key (see [`PowerDef::toggle`]).
    pub fn is_toggle(&self) -> bool {
        self.toggle
    }

    /// Needs a villain (or a spot) to aim at.
    pub fn aimed(&self) -> bool {
        matches!(
            self.delivery(),
            Delivery::Melee | Delivery::Ranged | Delivery::Line | Delivery::Cone | Delivery::Sky
        )
    }
}

const F: Fields = Fields::NONE;

#[allow(clippy::too_many_arguments)]
const fn def(
    id: u8,
    name: &'static str,
    blurb: &'static str,
    tree: Tree,
    (row, col): (u8, u8),
    needs: &'static [&'static [u8]],
    fork: u8,
    base: Power,
    noun: &'static str,
    fields: Fields,
) -> PowerDef {
    PowerDef {
        id: PowerId(id),
        name,
        blurb,
        tree,
        row,
        col,
        needs,
        needs_any: 0,
        fork,
        helper: false,
        capstone: false,
        toggle: false,
        base,
        noun,
        fields,
    }
}

const fn helper(mut d: PowerDef) -> PowerDef {
    d.helper = true;
    d
}

const fn capstone(mut d: PowerDef) -> PowerDef {
    d.capstone = true;
    d
}

/// A toggle taking `upkeep` of the hero's energy recovery while on.
const fn toggle(mut d: PowerDef, upkeep: f32) -> PowerDef {
    d.toggle = true;
    d.fields.upkeep = upkeep;
    d
}

const fn any_of_tree(mut d: PowerDef, n: u8) -> PowerDef {
    d.needs_any = n;
    d
}

use Power as B;
use Tree::*;

// Shared shapes, so the trees read as variations.
const BOLT: Fields = Fields {
    damage: 36.0,
    range: 40.0,
    knock: 1.0,
    ..F
};
const HOLD: Fields = Fields {
    damage: 12.0,
    range: 30.0,
    hold: 7.0,
    ..F
};
const SHIELD: Fields = Fields {
    shield: 0.5,
    radius: 12.0,
    lasts: 18.0,
    ..F
};
const LIFT: Fields = Fields {
    damage: 60.0,
    range: 30.0,
    knock: 1.0,
    ..F
};
const BLAST: Fields = Fields {
    damage: 34.0,
    radius: 8.0,
    knock: 1.0,
    ..F
};
const MEND: Fields = Fields {
    heal: 30.0,
    radius: 14.0,
    ..F
};
const BEAM: Fields = Fields {
    damage: 50.0,
    range: 36.0,
    radius: 1.4,
    knock: 1.0,
    ..F
};
const METEOR: Fields = Fields {
    damage: 32.0,
    range: 45.0,
    radius: 5.5,
    delay: 1.2,
    knock: 1.0,
    wreck: 3.0,
    ..F
};
const NOVA: Fields = Fields {
    damage: 120.0,
    radius: 16.0,
    knock: 1.0,
    wreck: 2.0,
    ..F
};
const FREEZE: Fields = Fields {
    damage: 12.0,
    range: 16.0,
    radius: 0.42,
    freeze: 3.5,
    slow: 0.5,
    slow_secs: 6.0,
    ..F
};
const TELEPORT: Fields = Fields { range: 40.0, ..F };
const SIDEKICK: Fields = Fields {
    range: 40.0,
    summons: 1,
    summon_power: 1.0,
    ..F
};
const HYPNO: Fields = Fields {
    range: 30.0,
    charm: 8.0,
    ..F
};
const REGEN: Fields = Fields { regen: 0.02, ..F };
const RALLY: Fields = Fields { aura: 0.4, ..F };

/// Strike's number.
pub const STRIKE: PowerId = PowerId::STRIKE;

/// Every power there is.
pub static POWERS: &[PowerDef] = &[
    def(
        1,
        "Strike",
        "a punch, free",
        Basic,
        (0, 0),
        &[],
        0,
        B::Strike,
        "fist",
        Fields {
            damage: 20.0,
            range: 3.0,
            ..F
        },
    ),
    // ---- Travel (any origin) ----
    def(
        2,
        "Super Jump",
        "leaps over rooftops; lands with a stomp",
        Common,
        (0, 1),
        &[],
        0,
        B::SuperJump,
        "legs",
        Fields {
            travel: travel::JUMP,
            ..F
        },
    ),
    def(
        3,
        "Sprint",
        "runs twice as fast",
        Common,
        (0, 3),
        &[],
        0,
        B::SuperSpeed,
        "legs",
        Fields {
            travel: travel::SPRINT,
            ..F
        },
    ),
    def(
        4,
        "Glide",
        "hold jump to float down",
        Common,
        (1, 0),
        &[&[2]],
        0,
        B::SuperJump,
        "wind",
        Fields {
            travel: travel::GLIDE,
            ..F
        },
    ),
    def(
        5,
        "Flight",
        "flies anywhere",
        Common,
        (1, 1),
        &[&[2]],
        0,
        B::Flight,
        "wind",
        Fields {
            travel: travel::FLY,
            ..F
        },
    ),
    def(
        6,
        "Super Speed",
        "runs like the wind",
        Common,
        (1, 3),
        &[&[3]],
        0,
        B::SuperSpeed,
        "legs",
        Fields {
            travel: travel::SPEED,
            ..F
        },
    ),
    def(
        7,
        "Wall Run",
        "runs up walls",
        Common,
        (2, 2),
        &[&[6, 2]],
        0,
        B::SuperSpeed,
        "legs",
        Fields {
            travel: travel::WALL,
            ..F
        },
    ),
    any_of_tree(
        def(
            8,
            "Hero's Return",
            "home to Hero Hall",
            Common,
            (2, 4),
            &[],
            0,
            B::Teleport,
            "aether",
            Fields {
                lands: Lands::Home,
                ..F
            },
        ),
        2,
    ),
    // ---- Lab Accident ----
    def(
        10,
        "Atomic Ray",
        "bolt, knocks down",
        Science,
        (0, 1),
        &[],
        0,
        B::Bolt,
        "atomic",
        BOLT,
    ),
    def(
        11,
        "Scanner Goggles",
        "25% more on helpless foes",
        Science,
        (0, 3),
        &[],
        0,
        B::Vitality,
        "gadget",
        Fields { exploit: 0.25, ..F },
    ),
    def(
        12,
        "Ray Burst",
        "a cone of rays",
        Science,
        (1, 0),
        &[&[10]],
        0,
        B::Freeze,
        "atomic",
        Fields {
            damage: 26.0,
            range: 14.0,
            radius: 0.35,
            ..F
        },
    ),
    def(
        13,
        "Gravity Cuffs",
        "holds one 7 s",
        Science,
        (1, 1),
        &[&[10]],
        0,
        B::Hold,
        "gravity",
        HOLD,
    ),
    def(
        14,
        "Transmitter",
        "teleports 40 m",
        Science,
        (1, 2),
        &[&[11]],
        0,
        B::Teleport,
        "gadget",
        TELEPORT,
    ),
    def(
        15,
        "Force Field",
        "friends take half",
        Science,
        (1, 4),
        &[&[11]],
        0,
        B::Shield,
        "force",
        SHIELD,
    ),
    def(
        16,
        "Cryo Ray",
        "freezes a cone, douses",
        Science,
        (2, 0),
        &[&[12]],
        0,
        B::Freeze,
        "frost",
        FREEZE,
    ),
    def(
        17,
        "Sonic Boom",
        "blast, knocks down",
        Science,
        (2, 1),
        &[&[12], &[13]],
        0,
        B::Shockwave,
        "sound",
        BLAST,
    ),
    def(
        18,
        "Tin Pal",
        "robot sidekick",
        Science,
        (2, 2),
        &[&[13], &[14]],
        0,
        B::Sidekick,
        "robot",
        SIDEKICK,
    ),
    def(
        19,
        "Stasis Cage",
        "holds an area 4 s",
        Science,
        (2, 3),
        &[&[14], &[15]],
        0,
        B::Meteor,
        "force",
        Fields {
            damage: 8.0,
            range: 30.0,
            radius: 5.0,
            delay: 0.2,
            hold: 4.0,
            ..F
        },
    ),
    def(
        20,
        "Vital Ray",
        "heals friends near",
        Science,
        (2, 4),
        &[&[15]],
        0,
        B::Mend,
        "atomic",
        MEND,
    ),
    def(
        21,
        "Disintegrator",
        "beam through a line",
        Science,
        (3, 0),
        &[&[16], &[17]],
        0,
        B::Beam,
        "atomic",
        BEAM,
    ),
    def(
        22,
        "Gravity Well",
        "pulls a crowd in",
        Science,
        (3, 1),
        &[&[17]],
        0,
        B::Meteor,
        "gravity",
        Fields {
            damage: 18.0,
            range: 35.0,
            radius: 7.0,
            delay: 0.5,
            push: -6.0,
            hold: 2.0,
            ..F
        },
    ),
    def(
        23,
        "Hypno Spiral",
        "a minion fights for you",
        Science,
        (3, 2),
        &[&[18]],
        0,
        B::Hypno,
        "mind",
        HYPNO,
    ),
    def(
        24,
        "Reflector Field",
        "shield sends 25% back",
        Science,
        (3, 3),
        &[&[20]],
        1,
        B::Shield,
        "force",
        Fields {
            reflect: 0.25,
            ..SHIELD
        },
    ),
    def(
        25,
        "Repulsor Field",
        "shoves villains out",
        Science,
        (3, 4),
        &[&[20]],
        1,
        B::Shockwave,
        "force",
        Fields {
            damage: 12.0,
            radius: 8.0,
            knock: 1.0,
            push: 6.0,
            ..F
        },
    ),
    def(
        26,
        "Orbital Ray",
        "strike from the sky",
        Science,
        (4, 0),
        &[&[21]],
        0,
        B::Meteor,
        "atomic",
        METEOR,
    ),
    def(
        27,
        "Tractor Beam",
        "hurls cars from afar",
        Science,
        (4, 1),
        &[&[22]],
        0,
        B::Lift,
        "gravity",
        Fields {
            range: 40.0,
            ..LIFT
        },
    ),
    def(
        28,
        "Overclocked Pal",
        "one strong robot",
        Science,
        (4, 2),
        &[&[23]],
        2,
        B::Sidekick,
        "robot",
        Fields {
            summon_power: 2.0,
            ..SIDEKICK
        },
    ),
    def(
        29,
        "Twin Pals",
        "two small robots",
        Science,
        (4, 3),
        &[&[23]],
        2,
        B::Sidekick,
        "robot",
        Fields {
            summons: 2,
            summon_power: 0.75,
            ..SIDEKICK
        },
    ),
    capstone(def(
        30,
        "Atomic Nova",
        "huge blast, 16 m",
        Science,
        (5, 1),
        &[&[26], &[27]],
        3,
        B::Nova,
        "atomic",
        NOVA,
    )),
    capstone(def(
        31,
        "Singularity",
        "a crushing black hole",
        Science,
        (5, 2),
        &[&[27], &[28, 29]],
        3,
        B::Meteor,
        "gravity",
        Fields {
            damage: 90.0,
            range: 40.0,
            radius: 9.0,
            delay: 1.5,
            push: -10.0,
            hold: 3.0,
            knock: 1.0,
            wreck: 4.0,
            ..F
        },
    )),
    // ---- Ancient Relic ----
    def(
        32,
        "Arcane Bolt",
        "bolt, knocks down",
        Mystic,
        (0, 1),
        &[],
        0,
        B::Bolt,
        "arcane",
        BOLT,
    ),
    toggle(
        def(
            33,
            "Spirit Sight",
            "1 blow in 5 misses you",
            Mystic,
            (0, 3),
            &[],
            0,
            B::Invulnerable,
            "spirit",
            Fields { dodge: 0.2, ..F },
        ),
        0.2,
    ),
    def(
        34,
        "Binding Sigils",
        "roots hold one 7 s",
        Mystic,
        (1, 0),
        &[&[32]],
        0,
        B::Hold,
        "roots",
        HOLD,
    ),
    def(
        35,
        "Hex",
        "target takes 25% more",
        Mystic,
        (1, 1),
        &[&[32]],
        0,
        B::Bolt,
        "curse",
        Fields {
            damage: 16.0,
            range: 35.0,
            weaken: 0.25,
            lasts: 8.0,
            ..F
        },
    ),
    def(
        36,
        "Ghost Lantern",
        "villains near flee 4 s",
        Mystic,
        (1, 2),
        &[&[33]],
        0,
        B::Shockwave,
        "spirit",
        Fields {
            radius: 8.0,
            fear: 4.0,
            spare: 1.0,
            ..F
        },
    ),
    def(
        37,
        "Aether Step",
        "teleports 40 m",
        Mystic,
        (1, 3),
        &[&[33]],
        0,
        B::Teleport,
        "aether",
        TELEPORT,
    ),
    def(
        38,
        "Telekinesis",
        "lift and throw",
        Mystic,
        (1, 4),
        &[&[33]],
        0,
        B::Lift,
        "mind",
        LIFT,
    ),
    def(
        39,
        "Frost Sigil",
        "freezes a cone, douses",
        Mystic,
        (2, 0),
        &[&[34]],
        0,
        B::Freeze,
        "frost",
        FREEZE,
    ),
    def(
        40,
        "Charm",
        "a minion fights for you",
        Mystic,
        (2, 1),
        &[&[35], &[34]],
        0,
        B::Hypno,
        "mind",
        HYPNO,
    ),
    def(
        41,
        "Spirit Ward",
        "a strong barrier",
        Mystic,
        (2, 2),
        &[&[36]],
        0,
        B::Shield,
        "spirit",
        Fields {
            shield: 0.6,
            radius: 14.0,
            lasts: 10.0,
            ..F
        },
    ),
    def(
        42,
        "Void Grasp",
        "pulls in and burns",
        Mystic,
        (2, 3),
        &[&[37], &[38]],
        0,
        B::Meteor,
        "void",
        Fields {
            damage: 24.0,
            range: 30.0,
            radius: 5.0,
            delay: 0.3,
            push: -5.0,
            dot: 0.5,
            ..F
        },
    ),
    toggle(
        def(
            43,
            "Wraith Form",
            "takes 20% less",
            Mystic,
            (2, 4),
            &[&[38]],
            0,
            B::Invulnerable,
            "spirit",
            Fields { resist: 0.2, ..F },
        ),
        0.2,
    ),
    def(
        44,
        "Spirit Lance",
        "beam through a line",
        Mystic,
        (3, 0),
        &[&[39]],
        0,
        B::Beam,
        "spirit",
        BEAM,
    ),
    def(
        45,
        "Curse of Weakness",
        "an area takes 20% more",
        Mystic,
        (3, 1),
        &[&[40]],
        1,
        B::Meteor,
        "curse",
        Fields {
            damage: 10.0,
            range: 35.0,
            radius: 6.0,
            delay: 0.2,
            weaken: 0.2,
            lasts: 10.0,
            ..F
        },
    ),
    def(
        46,
        "Circle of Sigils",
        "holds an area 4 s",
        Mystic,
        (3, 2),
        &[&[40], &[41]],
        1,
        B::Meteor,
        "roots",
        Fields {
            damage: 8.0,
            range: 30.0,
            radius: 6.0,
            delay: 0.2,
            hold: 4.0,
            ..F
        },
    ),
    def(
        47,
        "Soul Siphon",
        "a cone that heals you",
        Mystic,
        (3, 3),
        &[&[42]],
        0,
        B::Freeze,
        "void",
        Fields {
            damage: 26.0,
            range: 12.0,
            radius: 0.4,
            drain: 0.4,
            ..F
        },
    ),
    def(
        48,
        "Aether Mend",
        "heals friends near",
        Mystic,
        (3, 4),
        &[&[43]],
        0,
        B::Mend,
        "aether",
        MEND,
    ),
    def(
        49,
        "Starfall",
        "strike from the sky",
        Mystic,
        (4, 0),
        &[&[44]],
        0,
        B::Meteor,
        "stars",
        METEOR,
    ),
    def(
        50,
        "Familiar",
        "a spirit hound fights",
        Mystic,
        (4, 1),
        &[&[45, 46]],
        0,
        B::Sidekick,
        "spirit",
        Fields {
            summon_power: 1.2,
            ..SIDEKICK
        },
    ),
    def(
        51,
        "Void Tendrils",
        "holds a crowd 5 s",
        Mystic,
        (4, 3),
        &[&[47]],
        0,
        B::Meteor,
        "void",
        Fields {
            damage: 10.0,
            range: 30.0,
            radius: 8.0,
            delay: 0.3,
            hold: 5.0,
            ..F
        },
    ),
    toggle(
        def(
            52,
            "Aether Vitality",
            "2% health a second",
            Mystic,
            (4, 4),
            &[&[48]],
            0,
            B::Vitality,
            "aether",
            REGEN,
        ),
        0.25,
    ),
    capstone(def(
        53,
        "Eclipse",
        "stars rain for 8 s",
        Mystic,
        (5, 1),
        &[&[49], &[50]],
        2,
        B::Meteor,
        "stars",
        Fields {
            damage: 30.0,
            range: 40.0,
            radius: 6.0,
            delay: 1.0,
            repeats: 8,
            knock: 1.0,
            wreck: 1.0,
            ..F
        },
    )),
    capstone(def(
        54,
        "Void Rift",
        "swallows a street",
        Mystic,
        (5, 2),
        &[&[51], &[52]],
        2,
        B::Meteor,
        "void",
        Fields {
            damage: 80.0,
            range: 40.0,
            radius: 12.0,
            delay: 1.0,
            push: -8.0,
            knock: 1.0,
            wreck: 5.0,
            ..F
        },
    )),
    // ---- Born Mighty ----
    def(
        55,
        "Haymaker",
        "big punch, knocks down",
        Might,
        (0, 1),
        &[],
        0,
        B::Strike,
        "fist",
        Fields {
            damage: 34.0,
            range: 3.2,
            knock: 1.0,
            ..F
        },
    ),
    toggle(
        def(
            56,
            "Brace",
            "standing still: 30% less",
            Might,
            (0, 3),
            &[],
            0,
            B::Invulnerable,
            "stone",
            Fields { brace: 0.3, ..F },
        ),
        0.15,
    ),
    def(
        57,
        "Uppercut",
        "launches a villain",
        Might,
        (1, 0),
        &[&[55]],
        0,
        B::Strike,
        "fist",
        Fields {
            damage: 30.0,
            range: 3.2,
            knock: 1.0,
            launch: 9.0,
            ..F
        },
    ),
    def(
        58,
        "Thunderclap",
        "blast, knocks down",
        Might,
        (1, 1),
        &[&[55]],
        0,
        B::Shockwave,
        "sound",
        BLAST,
    ),
    def(
        59,
        "Mighty Heave",
        "lift and throw",
        Might,
        (1, 2),
        &[&[55], &[56]],
        0,
        B::Lift,
        "fist",
        LIFT,
    ),
    toggle(
        def(
            60,
            "Tough",
            "takes 15% less",
            Might,
            (1, 3),
            &[&[56]],
            0,
            B::Invulnerable,
            "skin",
            Fields { resist: 0.15, ..F },
        ),
        0.15,
    ),
    def(
        61,
        "Second Wind",
        "near defeat: 40% health back",
        Might,
        (1, 4),
        &[&[56]],
        0,
        B::Vitality,
        "stone",
        Fields {
            second_wind: 0.4,
            ..F
        },
    ),
    def(
        62,
        "Shoulder Charge",
        "dashes, bowls over",
        Might,
        (2, 0),
        &[&[57]],
        0,
        B::Teleport,
        "fist",
        Fields {
            range: 20.0,
            damage: 30.0,
            knock: 1.0,
            lands: Lands::Path,
            ..F
        },
    ),
    def(
        63,
        "Stomp",
        "slows everyone round",
        Might,
        (2, 1),
        &[&[58]],
        0,
        B::Shockwave,
        "stone",
        Fields {
            damage: 15.0,
            radius: 9.0,
            slow: 0.4,
            slow_secs: 6.0,
            ..F
        },
    ),
    def(
        64,
        "Lamp-post Swing",
        "sweeps all round you",
        Might,
        (2, 2),
        &[&[59]],
        0,
        B::Shockwave,
        "steel",
        Fields {
            damage: 30.0,
            radius: 4.5,
            knock: 1.0,
            ..F
        },
    ),
    def(
        65,
        "Boulder Toss",
        "rips up the street",
        Might,
        (2, 3),
        &[&[59], &[60]],
        0,
        B::Meteor,
        "stone",
        Fields {
            damage: 45.0,
            range: 35.0,
            radius: 4.0,
            delay: 0.8,
            knock: 1.0,
            wreck: 2.0,
            ..F
        },
    ),
    toggle(
        def(
            66,
            "Iron Constitution",
            "2% health a second",
            Might,
            (2, 4),
            &[&[60], &[61]],
            0,
            B::Vitality,
            "skin",
            REGEN,
        ),
        0.25,
    ),
    def(
        67,
        "Combo Flurry",
        "three quick blows",
        Might,
        (3, 0),
        &[&[62]],
        0,
        B::Strike,
        "fist",
        Fields {
            damage: 14.0,
            range: 3.2,
            repeats: 3,
            ..F
        },
    ),
    def(
        68,
        "Fissure",
        "cracks a line",
        Might,
        (3, 1),
        &[&[63]],
        0,
        B::Beam,
        "stone",
        Fields {
            damage: 40.0,
            range: 20.0,
            radius: 1.6,
            knock: 1.0,
            wreck: 1.0,
            ..F
        },
    ),
    def(
        69,
        "Giant Swing",
        "flings a villain far",
        Might,
        (3, 2),
        &[&[64], &[65]],
        0,
        B::Strike,
        "fist",
        Fields {
            damage: 45.0,
            range: 3.2,
            knock: 1.0,
            push: 10.0,
            ..F
        },
    ),
    toggle(
        def(
            70,
            "Berserk",
            "30% harder, 15% softer",
            Might,
            (3, 3),
            &[&[66]],
            1,
            B::Rally,
            "rage",
            Fields {
                might: 0.3,
                resist: -0.15,
                ..F
            },
        ),
        0.2,
    ),
    toggle(
        def(
            71,
            "Bulwark Stance",
            "never knocked down",
            Might,
            (3, 4),
            &[&[66]],
            1,
            B::Invulnerable,
            "stone",
            Fields {
                steadfast: 1.0,
                resist: 0.1,
                ..F
            },
        ),
        0.15,
    ),
    def(
        72,
        "Meteor Leap",
        "jump-slam from afar",
        Might,
        (4, 0),
        &[&[67]],
        0,
        B::Teleport,
        "stone",
        Fields {
            range: 30.0,
            damage: 40.0,
            radius: 5.0,
            knock: 1.0,
            lands: Lands::Arrival,
            wreck: 1.0,
            ..F
        },
    ),
    def(
        73,
        "Car Slam",
        "hurls a car",
        Might,
        (4, 1),
        &[&[68], &[69]],
        0,
        B::Lift,
        "steel",
        Fields {
            damage: 90.0,
            ..LIFT
        },
    ),
    toggle(
        def(
            74,
            "Steel Hide",
            "takes half",
            Might,
            (4, 3),
            &[&[70, 71]],
            0,
            B::Invulnerable,
            "steel",
            Fields { resist: 0.5, ..F },
        ),
        0.4,
    ),
    capstone(def(
        75,
        "Earthshaker",
        "huge blast, 16 m",
        Might,
        (5, 1),
        &[&[72], &[73]],
        2,
        B::Nova,
        "stone",
        Fields { wreck: 3.0, ..NOVA },
    )),
    capstone(def(
        76,
        "Unstoppable",
        "a titan for 20 s",
        Might,
        (5, 2),
        &[&[73], &[74]],
        2,
        B::Shield,
        "rage",
        Fields {
            resist: 0.5,
            might: 0.5,
            steadfast: 1.0,
            lasts: 20.0,
            ..F
        },
    )),
    // ---- Sworn Guardian ----
    def(
        77,
        "Light Lance",
        "bolt of light",
        Guardian,
        (0, 1),
        &[],
        0,
        B::Bolt,
        "light",
        BOLT,
    ),
    def(
        78,
        "Guardian Orb",
        "shields you 40%, 8 s",
        Guardian,
        (0, 3),
        &[],
        0,
        B::Shield,
        "light",
        Fields {
            shield: 0.4,
            lasts: 8.0,
            ..F
        },
    ),
    def(
        79,
        "Shield Bash",
        "punch, knocks down",
        Guardian,
        (1, 0),
        &[&[77]],
        0,
        B::Strike,
        "shield",
        Fields {
            damage: 30.0,
            range: 3.2,
            knock: 1.0,
            ..F
        },
    ),
    def(
        80,
        "Healing Light",
        "heals friends near",
        Guardian,
        (1, 1),
        &[&[77]],
        0,
        B::Mend,
        "light",
        MEND,
    ),
    helper(def(
        81,
        "Lifeline",
        "revives from 25 m; gets you up",
        Guardian,
        (1, 2),
        &[&[78]],
        0,
        B::Vitality,
        "light",
        Fields {
            revive_reach: 25.0,
            get_up: 10.0,
            ..F
        },
    )),
    def(
        82,
        "Guardian Aegis",
        "friends take half",
        Guardian,
        (1, 3),
        &[&[78]],
        0,
        B::Shield,
        "light",
        SHIELD,
    ),
    def(
        83,
        "Protector's Bond",
        "a strong barrier",
        Guardian,
        (1, 4),
        &[&[78]],
        0,
        B::Shield,
        "light",
        Fields {
            shield: 0.6,
            radius: 14.0,
            lasts: 10.0,
            ..F
        },
    ),
    def(
        84,
        "Shield Throw",
        "bounces to three",
        Guardian,
        (2, 0),
        &[&[79]],
        0,
        B::Bolt,
        "shield",
        Fields {
            damage: 26.0,
            range: 30.0,
            spread: 1.0,
            spread_to: 2,
            ..F
        },
    ),
    def(
        85,
        "Halo Burst",
        "blast that heals",
        Guardian,
        (2, 1),
        &[&[79], &[80]],
        0,
        B::Shockwave,
        "light",
        Fields {
            damage: 24.0,
            radius: 8.0,
            knock: 1.0,
            heal: 20.0,
            ..F
        },
    ),
    def(
        86,
        "Ring of Judgement",
        "foes round you take 15% more",
        Guardian,
        (2, 2),
        &[&[81]],
        0,
        B::Shockwave,
        "light",
        Fields {
            damage: 20.0,
            radius: 6.0,
            weaken: 0.15,
            lasts: 8.0,
            ..F
        },
    ),
    def(
        87,
        "Sanctuary",
        "heals friends, slows foes",
        Guardian,
        (2, 3),
        &[&[82]],
        0,
        B::Mend,
        "light",
        Fields {
            heal: 15.0,
            radius: 10.0,
            slow: 0.4,
            slow_secs: 6.0,
            ..F
        },
    ),
    def(
        88,
        "Intercept",
        "leaps in, shields there",
        Guardian,
        (2, 4),
        &[&[83]],
        0,
        B::Teleport,
        "light",
        Fields {
            range: 30.0,
            lands: Lands::Arrival,
            shield: 0.5,
            radius: 5.0,
            lasts: 6.0,
            ..F
        },
    ),
    def(
        89,
        "Radiant Beam",
        "beam through a line",
        Guardian,
        (3, 0),
        &[&[84]],
        0,
        B::Beam,
        "light",
        BEAM,
    ),
    def(
        90,
        "Light Pillar",
        "a burning column",
        Guardian,
        (3, 1),
        &[&[85]],
        0,
        B::Meteor,
        "light",
        Fields {
            damage: 12.0,
            range: 30.0,
            radius: 5.0,
            dot: 1.0,
            repeats: 4,
            ..F
        },
    ),
    toggle(
        def(
            91,
            "Steady Heart",
            "2% health a second",
            Guardian,
            (3, 2),
            &[&[86], &[87]],
            0,
            B::Vitality,
            "light",
            REGEN,
        ),
        0.25,
    ),
    toggle(
        def(
            92,
            "Reflective Aegis",
            "sends 20% of blows back",
            Guardian,
            (3, 3),
            &[&[87]],
            1,
            B::Invulnerable,
            "light",
            Fields { reflect: 0.2, ..F },
        ),
        0.15,
    ),
    toggle(
        def(
            93,
            "Mobile Aegis",
            "friends near take 15% less",
            Guardian,
            (3, 4),
            &[&[88]],
            1,
            B::Rally,
            "light",
            Fields { ward: 0.15, ..F },
        ),
        0.15,
    ),
    toggle(
        def(
            94,
            "Heroic Aura",
            "friends near hit harder",
            Guardian,
            (4, 1),
            &[&[90]],
            0,
            B::Rally,
            "light",
            RALLY,
        ),
        0.25,
    ),
    def(
        95,
        "Guardian Angel",
        "survives a fatal blow",
        Guardian,
        (4, 2),
        &[&[91]],
        2,
        B::Invulnerable,
        "light",
        Fields { angel: 1.0, ..F },
    ),
    toggle(
        def(
            96,
            "Martyr",
            "friends near take 25% less",
            Guardian,
            (4, 3),
            &[&[91]],
            2,
            B::Rally,
            "light",
            Fields { ward: 0.25, ..F },
        ),
        0.25,
    ),
    def(
        97,
        "Bulwark of Light",
        "a big barrier",
        Guardian,
        (4, 4),
        &[&[92, 93]],
        0,
        B::Shield,
        "light",
        Fields {
            shield: 0.75,
            radius: 14.0,
            lasts: 12.0,
            ..F
        },
    ),
    capstone(def(
        98,
        "Dawn",
        "heals and revives all",
        Guardian,
        (5, 2),
        &[&[94], &[95, 96]],
        3,
        B::Mend,
        "light",
        Fields {
            heal: 100.0,
            radius: 30.0,
            revive: 1.0,
            ..F
        },
    )),
    capstone(def(
        99,
        "Last Bastion",
        "a dome blows bounce off",
        Guardian,
        (5, 3),
        &[&[95, 96], &[97]],
        3,
        B::Shield,
        "light",
        Fields {
            shield: 0.9,
            radius: 10.0,
            lasts: 10.0,
            ..F
        },
    )),
    // ---- Masked Avenger ----
    def(
        100,
        "One-Two",
        "a quick double blow",
        Justice,
        (0, 1),
        &[],
        0,
        B::Strike,
        "fist",
        Fields {
            damage: 16.0,
            range: 3.2,
            repeats: 2,
            ..F
        },
    ),
    def(
        101,
        "Opening Move",
        "first blow on the unwary: double",
        Justice,
        (0, 3),
        &[],
        0,
        B::Vitality,
        "wits",
        Fields { ambush: 1.0, ..F },
    ),
    def(
        102,
        "Disarm",
        "gunmen can't shoot 6 s",
        Justice,
        (1, 0),
        &[&[100]],
        0,
        B::Strike,
        "fist",
        Fields {
            damage: 14.0,
            range: 3.2,
            disarm: 6.0,
            ..F
        },
    ),
    def(
        103,
        "Combat Roll",
        "dodges almost all, 2 s",
        Justice,
        (1, 1),
        &[&[100]],
        0,
        B::Shield,
        "wits",
        Fields {
            resist: 0.9,
            lasts: 2.0,
            ..F
        },
    ),
    def(
        104,
        "Grapple Line",
        "swings you 35 m",
        Justice,
        (1, 2),
        &[&[100], &[101]],
        0,
        B::Teleport,
        "gadget",
        Fields { range: 35.0, ..F },
    ),
    def(
        105,
        "Smoke Bomb",
        "confuses, hides you",
        Justice,
        (1, 3),
        &[&[101]],
        0,
        B::Shockwave,
        "smoke",
        Fields {
            radius: 6.0,
            confuse: 4.0,
            hide: 1.0,
            lasts: 3.0,
            spare: 1.0,
            ..F
        },
    ),
    def(
        106,
        "Cuff 'Em",
        "beaten-down minions give up",
        Justice,
        (1, 4),
        &[&[101]],
        0,
        B::Vitality,
        "gadget",
        Fields {
            bounty: 0.25,
            cuff: 0.2,
            ..F
        },
    ),
    def(
        107,
        "Takedown",
        "crushes held foes",
        Justice,
        (2, 0),
        &[&[102]],
        0,
        B::Strike,
        "fist",
        Fields {
            damage: 30.0,
            range: 3.2,
            exploit: 1.0,
            ..F
        },
    ),
    def(
        108,
        "Sweep Kick",
        "knocks all round down",
        Justice,
        (2, 1),
        &[&[103]],
        0,
        B::Shockwave,
        "fist",
        Fields {
            damage: 18.0,
            radius: 4.0,
            knock: 1.0,
            spare: 1.0,
            ..F
        },
    ),
    def(
        109,
        "Bolas",
        "tangles one 4 s",
        Justice,
        (2, 2),
        &[&[104]],
        0,
        B::Hold,
        "gadget",
        Fields {
            damage: 10.0,
            range: 30.0,
            hold: 4.0,
            ..F
        },
    ),
    def(
        110,
        "Intimidate",
        "minions flee",
        Justice,
        (2, 3),
        &[&[105]],
        0,
        B::Shockwave,
        "wits",
        Fields {
            radius: 10.0,
            fear: 4.0,
            spare: 1.0,
            ..F
        },
    ),
    def(
        111,
        "Shadow Cloak",
        "villains lose you 8 s",
        Justice,
        (2, 4),
        &[&[105], &[106]],
        0,
        B::Shield,
        "smoke",
        Fields {
            hide: 1.0,
            lasts: 8.0,
            ..F
        },
    ),
    def(
        112,
        "Trusty Partner",
        "a masked friend fights",
        Justice,
        (3, 0),
        &[&[107]],
        0,
        B::Sidekick,
        "wits",
        SIDEKICK,
    ),
    def(
        113,
        "Martial Master",
        "punches leap on",
        Justice,
        (3, 1),
        &[&[108]],
        1,
        B::Rally,
        "fist",
        Fields { chain: 0.3, ..F },
    ),
    def(
        114,
        "Gadget Belt",
        "powers recharge 30% faster",
        Justice,
        (3, 2),
        &[&[109]],
        1,
        B::Rally,
        "gadget",
        Fields { haste: 0.3, ..F },
    ),
    def(
        115,
        "Flash Bang",
        "confuses a crowd",
        Justice,
        (3, 3),
        &[&[110]],
        0,
        B::Shockwave,
        "smoke",
        Fields {
            damage: 5.0,
            radius: 8.0,
            confuse: 4.0,
            ..F
        },
    ),
    toggle(
        def(
            116,
            "Unbreakable Will",
            "steadfast, 20% less",
            Justice,
            (3, 4),
            &[&[111]],
            0,
            B::Invulnerable,
            "wits",
            Fields {
                steadfast: 1.0,
                resist: 0.2,
                ..F
            },
        ),
        0.2,
    ),
    def(
        117,
        "Zip Line Strike",
        "dashes through a line",
        Justice,
        (4, 0),
        &[&[112]],
        0,
        B::Teleport,
        "gadget",
        Fields {
            range: 25.0,
            damage: 30.0,
            knock: 1.0,
            lands: Lands::Path,
            ..F
        },
    ),
    def(
        118,
        "Stun Darts",
        "leap on to three",
        Justice,
        (4, 1),
        &[&[113, 114]],
        0,
        B::Bolt,
        "gadget",
        Fields {
            damage: 18.0,
            range: 30.0,
            spread: 1.0,
            spread_to: 2,
            slow: 0.3,
            slow_secs: 3.0,
            ..F
        },
    ),
    toggle(
        def(
            119,
            "Rallying Cry",
            "friends near hit harder",
            Justice,
            (4, 3),
            &[&[115]],
            0,
            B::Rally,
            "wits",
            RALLY,
        ),
        0.25,
    ),
    def(
        120,
        "Vanish",
        "villains lose you 10 s",
        Justice,
        (4, 4),
        &[&[116]],
        0,
        B::Shield,
        "smoke",
        Fields {
            hide: 1.0,
            lasts: 10.0,
            ..F
        },
    ),
    capstone(def(
        121,
        "Final Verdict",
        "minions fall at a blow, 10 s",
        Justice,
        (5, 2),
        &[&[117], &[118]],
        2,
        B::Shield,
        "wits",
        Fields {
            execute: 1.0,
            lasts: 10.0,
            ..F
        },
    )),
    capstone(def(
        122,
        "Reign of Fear",
        "every villain near flees",
        Justice,
        (5, 3),
        &[&[119], &[120]],
        2,
        B::Shockwave,
        "wits",
        Fields {
            radius: 25.0,
            fear: 8.0,
            spare: 1.0,
            ..F
        },
    )),
    // ---- The portal: every hero's, free (see `crate::gates`) ----
    def(
        124,
        "Portal Home",
        "marks where your portals lead",
        Basic,
        (0, 1),
        &[],
        0,
        B::Teleport,
        "aether",
        Fields {
            lands: Lands::GateMark,
            ..F
        },
    ),
    def(
        125,
        "Open Portal",
        "tears a portal open to your mark",
        Basic,
        (0, 2),
        &[],
        0,
        B::Teleport,
        "aether",
        Fields {
            lands: Lands::GateOpen,
            ..F
        },
    ),
];

/// The portal power's two buttons (every hero's, free).
pub const GATE_MARK: PowerId = PowerId::GATE_MARK;
pub const GATE_OPEN: PowerId = PowerId::GATE_OPEN;

/// Strike, Super Jump, Sprint and the portal's buttons: every hero owns
/// them, free.
pub fn is_free(id: PowerId) -> bool {
    PowerId::FREE.contains(&id)
}

/// The table from numbers to powers, built once.
fn table() -> &'static [Option<&'static PowerDef>; POWER_IDS] {
    static T: std::sync::OnceLock<[Option<&'static PowerDef>; POWER_IDS]> =
        std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut t = [None; POWER_IDS];
        for d in POWERS {
            t[d.id.0 as usize] = Some(d);
        }
        t
    })
}

/// The power with this number, if there is one.
pub fn get(id: PowerId) -> Option<&'static PowerDef> {
    table().get(id.0 as usize).copied().flatten()
}

/// The power with this number (Strike for an unknown one).
pub fn def_of(id: PowerId) -> &'static PowerDef {
    get(id).unwrap_or(&POWERS[0])
}

/// The powers of `tree`.
pub fn tree(tree: Tree) -> impl Iterator<Item = &'static PowerDef> {
    POWERS.iter().filter(move |d| d.tree == tree)
}

// ---- What a power costs to use ----

/// How many villains a power is reckoned to reach (an area by its size).
pub fn targets(d: &PowerDef) -> f32 {
    let f = &d.fields;
    let area = |r: f64| 1.0 + r as f32 / 4.0;
    match d.delivery() {
        Delivery::Melee | Delivery::Ranged => 1.0 + f.spread * f.spread_to as f32,
        Delivery::Line | Delivery::Cone => 2.0,
        Delivery::Burst | Delivery::Sky => area(f.radius),
        Delivery::Blink if f.lands == Lands::Path => 2.0,
        Delivery::Blink => area(f.radius),
        _ => 1.0,
    }
}

/// How much a power does, in one number: what energy and recharge follow
/// from. Damage counts by how many it reaches, control by the seconds it
/// takes away, help by how much and how many.
pub fn value(d: &PowerDef) -> f32 {
    let f = &d.fields;
    let targets = targets(d);
    let hits = f.repeats.max(1) as f32;
    let harm = f.damage * (1.0 + f.dot + f.drain + f.exploit * 0.5) * hits * targets;
    let control = (f.hold as f32 * 8.0
        + f.freeze as f32 * 5.0
        + f.confuse as f32 * 8.0
        + f.fear as f32 * 7.0
        + f.charm as f32 * 9.0
        + f.disarm as f32 * 5.0
        + f.slow * f.slow_secs as f32 * 4.0
        + f.weaken * f.lasts as f32 * 20.0
        + f.push.abs()
        + f.launch * 0.5)
        * targets;
    let friends = 1.0 + f.radius.min(18.0) as f32 / 6.0;
    let lasting = f.lasts as f32;
    let help = (f.heal
        + f.shield * lasting * 4.0
        + f.reflect * lasting * 10.0
        + (f.resist.max(0.0) + f.might + f.haste) * lasting * 25.0
        + f.steadfast * lasting * 2.0
        + f.hide * lasting * 6.0
        + f.execute * lasting * 15.0
        + f.revive * 100.0)
        * friends;
    let moving = if d.delivery() == Delivery::Blink {
        f.range as f32 * 1.5 + if f.lands == Lands::Home { 60.0 } else { 0.0 }
    } else {
        0.0
    };
    let summon = f.summons as f32 * f.summon_power * 150.0;
    let rescue = f.rescues as f32 * 100.0;
    harm + control + help + moving + summon + rescue + f.wreck * 5.0
}

/// Energy a power takes (0 for Strike, passive powers and toggles: a
/// toggle costs recovery instead, see [`Fields::upkeep`]).
pub fn energy(d: &PowerDef) -> f32 {
    if is_free(d.id) || d.passive() || d.toggle {
        return 0.0;
    }
    (2.0 + 0.6 * value(d).powf(0.75)).round()
}

/// Seconds before a power can be used again.
pub fn recharge(d: &PowerDef) -> f64 {
    if d.id == STRIKE {
        return 0.7;
    }
    match d.fields.lands {
        Lands::GateMark => return GATE_MARK_RECHARGE,
        Lands::GateOpen => return GATE_OPEN_RECHARGE,
        _ => {}
    }
    if d.passive() {
        return 0.0;
    }
    if d.toggle {
        return TOGGLE_RECHARGE;
    }
    let f = &d.fields;
    // Area hits wait longer (a group down in one go is worth the wait), but
    // the wait never grows past 45 s that way.
    let area = match d.delivery() {
        Delivery::Burst | Delivery::Sky => 2.5,
        Delivery::Line | Delivery::Cone => 1.4,
        _ => 1.0,
    };
    let base = 0.025 * (value(d) as f64).powf(1.15);
    let by_value = (base * area).min(base.max(45.0));
    // Never sooner than what it does wears off (a shield up for good, a
    // villain held for good).
    let floor = (f.lasts * 1.3)
        .max(f.hold * 0.8)
        .max(f.freeze * 4.0)
        .max(f.confuse * 2.0)
        .max(f.fear * 2.0)
        .max(f.charm * 3.5)
        .max(f.disarm * 2.0)
        .max(f.heal as f64 * 0.5)
        .max(if f.summons > 0 { 30.0 } else { 0.0 })
        .max(if f.repeats > 1 && d.delivery() == Delivery::Sky {
            f.repeats as f64 * 2.0
        } else {
            0.0
        })
        .max(1.0);
    ((by_value.max(floor)) * 10.0).round() / 10.0
}

/// Seconds before the portal's buttons can be pressed again.
pub const GATE_MARK_RECHARGE: f64 = 2.0;
pub const GATE_OPEN_RECHARGE: f64 = 4.0;

/// Seconds before a toggle can be switched again.
pub const TOGGLE_RECHARGE: f64 = 1.0;

// ---- Elements ----

/// One per hero, picked at creation, for good: it rides on every blow.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Element {
    /// Not picked yet (an old hero file): nothing rides.
    #[default]
    None,
    /// A burn: 15% of the blow again over 3 s.
    Fire,
    /// Slowed 15% for 3 s.
    Frost,
    /// Shoved back 1 m.
    Energy,
    /// 5% of the blow comes back as health.
    Necrotic,
    /// 20% leaps to one more villain within 5 m.
    Lightning,
    /// One in five blows: the villain wanders confused for 1 s (once in 4 s).
    Psychic,
}

impl Element {
    pub const ALL: [Element; 6] = [
        Element::Fire,
        Element::Frost,
        Element::Energy,
        Element::Necrotic,
        Element::Lightning,
        Element::Psychic,
    ];

    pub fn code(self) -> u8 {
        self as u8
    }

    pub fn from_code(c: u8) -> Element {
        Self::ALL
            .into_iter()
            .find(|e| e.code() == c)
            .unwrap_or(Element::None)
    }

    pub fn name(self) -> &'static str {
        match self {
            Element::None => "None",
            Element::Fire => "Fire",
            Element::Frost => "Frost",
            Element::Energy => "Energy",
            Element::Necrotic => "Necrotic",
            Element::Lightning => "Lightning",
            Element::Psychic => "Psychic",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Element::None => "nothing rides on your blows yet",
            Element::Fire => "every blow burns on: 15% more over 3 s",
            Element::Frost => "every blow slows a villain 15% for 3 s",
            Element::Energy => "every blow shoves a villain back a metre",
            Element::Necrotic => "every blow gives you 5% of it back as health",
            Element::Lightning => "20% of every blow leaps to a villain near",
            Element::Psychic => "one blow in five leaves a villain confused for a second",
        }
    }
}

/// Element numbers.
pub const FIRE_BURN: f32 = 0.15;
pub const FROST_SLOW: f32 = 0.15;
pub const FROST_SECONDS: f64 = 3.0;
pub const ENERGY_PUSH: f32 = 1.0;
pub const NECROTIC_DRAIN: f32 = 0.05;
pub const LIGHTNING_SHARE: f32 = 0.2;
pub const LIGHTNING_REACH: f64 = 5.0;
pub const PSYCHIC_CHANCE: f64 = 0.2;
pub const PSYCHIC_SECONDS: f64 = 1.0;
pub const PSYCHIC_COOLDOWN: f64 = 4.0;

// ---- A hero's kit ----

/// The powers a hero owns and their element, with the passive ones (and
/// the toggles switched on) summed, small enough to copy about every tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kit {
    owned: u128,
    /// The toggles switched on.
    on: u128,
    pub element: Element,
    /// Every owned passive power's fields and every toggle's that is on,
    /// summed.
    pub passive: Fields,
    /// The power the blast key fires (the hero's first bolt), if any.
    pub blast: Option<PowerId>,
    /// The augments in the powers' sockets (see [`crate::augments`]).
    sockets: Sockets,
}

/// Powers with augments a kit holds: every power a hero can own.
const SOCKET_ROWS: usize = hw_net::protocol::MAX_SET_AUGMENTS / SOCKETS;

/// The augments set into a hero's powers: a short list of powers and
/// their three sockets.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Sockets {
    rows: [(PowerId, [Augment; SOCKETS]); SOCKET_ROWS],
    len: u8,
}

impl Sockets {
    const NONE: Sockets = Sockets {
        rows: [(PowerId(0), [Augment::NONE; SOCKETS]); SOCKET_ROWS],
        len: 0,
    };

    fn of(&self, id: PowerId) -> [Augment; SOCKETS] {
        self.rows[..self.len as usize]
            .iter()
            .find(|r| r.0 == id)
            .map_or([Augment::NONE; SOCKETS], |r| r.1)
    }

    /// Puts `a` into socket `slot` of `id` (nothing happens past the
    /// last row: a sane sheet never has that many powers).
    fn put(&mut self, id: PowerId, slot: usize, a: Augment) {
        if slot >= SOCKETS {
            return;
        }
        let n = self.len as usize;
        let row = match self.rows[..n].iter().position(|r| r.0 == id) {
            Some(i) => i,
            None if n < SOCKET_ROWS => {
                self.rows[n] = (id, [Augment::NONE; SOCKETS]);
                self.len += 1;
                n
            }
            None => return,
        };
        self.rows[row].1[slot] = a;
    }
}

impl Default for Kit {
    fn default() -> Self {
        Self::bare(Sockets::NONE, 0, Element::None)
    }
}

impl Kit {
    /// A kit with only the free powers, these augments and these toggles on.
    fn bare(sockets: Sockets, on: u128, element: Element) -> Self {
        let mut k = Kit {
            owned: 0,
            on,
            element,
            passive: Fields::NONE,
            blast: None,
            sockets,
        };
        // The free powers count like bought ones (Super Jump and Sprint
        // bring their travel bits).
        for p in PowerId::FREE {
            k.add(p);
        }
        k
    }

    pub fn of(sheet: &Sheet) -> Self {
        let mut sockets = Sockets::NONE;
        for a in &sheet.augments {
            if sheet.has(a.power) {
                sockets.put(a.power, a.slot as usize, a.augment);
            }
        }
        let mut k = Kit::bare(sockets, 0, Element::from_code(sheet.element));
        for p in &sheet.powers {
            k.add(p.power);
        }
        k
    }

    /// This kit with `augments` in the sockets of `id` (tests and tools).
    pub fn with_augments(self, id: PowerId, augments: &[Augment]) -> Self {
        let mut sockets = self.sockets;
        for (slot, a) in augments.iter().enumerate().take(SOCKETS) {
            sockets.put(id, slot, *a);
        }
        let owned: Vec<PowerId> = self.owned().collect();
        let mut k = Kit::bare(sockets, self.on, self.element);
        for id in owned {
            k.add(id);
        }
        k
    }

    /// The augments in the sockets of `id`.
    pub fn sockets(&self, id: PowerId) -> [Augment; SOCKETS] {
        self.sockets.of(id)
    }

    /// What augments add to travel, in whole percent: speed on the ground,
    /// flying, through the air without flying, and a super jump's height
    /// (see [`crate::Moves::boosts`]).
    pub fn boosts(&self) -> [u8; 4] {
        let p = &self.passive;
        [p.swift, p.soar, p.leap, p.height].map(|x| (x * 100.0).round().clamp(0.0, 255.0) as u8)
    }

    /// What power `id` does in this hero's hands: its fields with the
    /// augments in its sockets added (see [`crate::augments::apply`]).
    pub fn fields(&self, id: PowerId) -> Fields {
        let d = def_of(id);
        if self.sockets.len == 0 {
            return d.fields;
        }
        crate::augments::apply(d, &self.sockets.of(id))
    }

    /// Energy power `id` takes in this hero's hands.
    pub fn energy(&self, id: PowerId) -> f32 {
        crate::augments::energy(def_of(id), &self.fields(id))
    }

    /// Seconds before power `id` is ready again in this hero's hands
    /// (before any haste).
    pub fn recharge(&self, id: PowerId) -> f64 {
        crate::augments::recharge(def_of(id), &self.fields(id))
    }

    fn add(&mut self, id: PowerId) {
        let Some(d) = get(id) else {
            return;
        };
        // Once only: a power's passive fields count once.
        if self.has(id) {
            return;
        }
        self.owned |= 1 << id.0;
        if d.passive() || (d.toggle && self.is_on(id)) {
            let fields = self.fields(id);
            self.passive.add_passive(&fields);
        }
        let first_bolt = self.blast.is_none_or(|b| {
            let o = def_of(b);
            (d.row, d.id) < (o.row, o.id)
        });
        if d.base == Power::Bolt && first_bolt {
            self.blast = Some(id);
        }
    }

    /// The given powers (tests and tools).
    pub fn with(ids: &[PowerId]) -> Self {
        let mut k = Kit::default();
        for &id in ids {
            k.add(id);
        }
        k
    }

    /// Every power (villain-free tests, a cheat).
    pub fn everything() -> Self {
        let mut k = Kit::default();
        for d in POWERS {
            k.add(d.id);
        }
        k
    }

    pub fn has(self, id: PowerId) -> bool {
        id.0 < 128 && self.owned & (1 << id.0) != 0
    }

    /// The owned powers, by number.
    pub fn owned(self) -> impl Iterator<Item = PowerId> {
        (0..128u8)
            .filter(move |i| self.owned & (1 << i) != 0)
            .map(PowerId)
    }

    pub fn travels(self, bit: u8) -> bool {
        self.passive.travel & bit != 0
    }

    /// Toggle `id` is owned and switched on.
    pub fn is_on(self, id: PowerId) -> bool {
        id.0 < 128 && self.on & (1 << id.0) != 0
    }

    /// The toggles switched on, as bits by power number.
    pub fn toggles_on(self) -> u128 {
        self.on
    }

    /// Switches the toggles in `on` on and every other one off (only
    /// owned toggles count).
    pub fn set_toggles(&mut self, on: u128) {
        let owned: Vec<PowerId> = self.owned().collect();
        let on = on & self.owned & toggle_bits();
        let mut k = Kit::bare(self.sockets, on, self.element);
        for id in owned {
            k.add(id);
        }
        *self = k;
    }

    /// Switches toggle `id` (an owned one) on or off; true when it is on now.
    pub fn flip(&mut self, id: PowerId) -> bool {
        self.set_toggles(self.on ^ (1 << id.0));
        self.is_on(id)
    }

    /// The share of energy recovery the toggles switched on take (0 to 1).
    pub fn upkeep(self) -> f32 {
        self.passive.upkeep.clamp(0.0, 1.0)
    }
}

/// Every toggle, as bits by power number.
fn toggle_bits() -> u128 {
    POWERS
        .iter()
        .filter(|d| d.toggle)
        .fold(0, |m, d| m | 1 << d.id.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_unique_and_links_point_up_one_row() {
        let mut seen = std::collections::BTreeSet::new();
        for d in POWERS {
            assert!(seen.insert(d.id), "{} twice", d.id.0);
            assert!((d.id.0 as usize) < POWER_IDS);
            for any in d.needs {
                for &n in *any {
                    let o = get(PowerId(n)).unwrap_or_else(|| panic!("{} needs {n}", d.name));
                    assert_eq!(o.tree, d.tree, "{} needs {}", d.name, o.name);
                    assert!(o.row < d.row, "{} needs {} below it", d.name, o.name);
                }
            }
            assert!(d.name.len() <= 20, "{}", d.name);
        }
        // Fork partners sit side by side in one row.
        for d in POWERS.iter().filter(|d| d.fork != 0) {
            let partners: Vec<_> = POWERS
                .iter()
                .filter(|o| o.tree == d.tree && o.fork == d.fork && o.id != d.id)
                .collect();
            assert_eq!(partners.len(), 1, "{}", d.name);
            assert_eq!(partners[0].row, d.row);
        }
    }

    #[test]
    fn every_origin_tree_has_twenty_odd_powers_and_two_capstones() {
        for t in Tree::ORIGIN_TREES {
            let n = tree(t).count();
            assert!((20..=24).contains(&n), "{t:?}: {n}");
            let caps: Vec<_> = tree(t).filter(|d| d.capstone).collect();
            assert_eq!(caps.len(), 2, "{t:?}");
            assert_eq!(caps[0].fork, caps[1].fork);
        }
    }

    #[test]
    fn the_old_powers_cost_about_what_they_did() {
        // (power, energy then, recharge then)
        let old = [
            (10, 10.0, 1.6),
            (13, 12.0, 6.0),
            (15, 16.0, 26.0),
            (17, 26.0, 12.0),
            (21, 22.0, 7.0),
            (26, 30.0, 18.0),
            (30, 60.0, 40.0),
            (16, 14.0, 16.0),
            (23, 20.0, 30.0),
        ];
        for (id, e, r) in old {
            let d = def_of(PowerId(id));
            let (ne, nr) = (energy(d), recharge(d));
            assert!(
                ne >= e * 0.5 && ne <= e * 1.8,
                "{}: energy {ne} (was {e})",
                d.name
            );
            assert!(
                nr >= r * 0.4 && nr <= r * 2.0,
                "{}: recharge {nr} (was {r})",
                d.name
            );
        }
        assert_eq!(energy(def_of(STRIKE)), 0.0);
        for d in POWERS
            .iter()
            .filter(|d| !d.passive() && !d.toggle && !is_free(d.id))
        {
            assert!(
                energy(d) >= 3.0 && energy(d) <= 100.0,
                "{}: {}",
                d.name,
                energy(d)
            );
            assert!(
                recharge(d) >= 1.0 && recharge(d) <= 90.0,
                "{}: {}",
                d.name,
                recharge(d)
            );
        }
    }

    #[test]
    fn a_kit_sums_its_passives() {
        let mut k = Kit::with(&[PowerId(60), PowerId(74), PowerId(2), PowerId(10)]);
        // Toggles count only while on.
        assert_eq!(k.passive.resist, 0.0);
        k.set_toggles(u128::MAX);
        assert!((k.passive.resist - (1.0 - 0.85 * 0.5)).abs() < 1e-5);
        assert!((k.upkeep() - 0.55).abs() < 1e-5);
        assert_eq!(k.toggles_on(), 1 << 60 | 1 << 74);
        assert!(!k.flip(PowerId(60)) && k.is_on(PowerId(74)));
        assert!(k.travels(travel::JUMP) && !k.travels(travel::FLY));
        assert_eq!(k.blast, Some(PowerId(10)));
        assert!(k.has(STRIKE));
        assert!(Kit::everything().has(PowerId(122)));
    }
}
