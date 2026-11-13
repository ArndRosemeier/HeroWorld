//! Augments: loot that makes a power do a little more.
//!
//! An augment ([`hw_net::Augment`]) adds a percentage of one [`Effect`] to
//! the power it is set into. A power that has the effect already gets
//! better at it; one that does not gains it in a weak form (a bolt that
//! also holds for a moment). Every power has three sockets
//! ([`hw_net::SOCKETS`]), which must hold three different effects. An
//! augment is set for good: another set into its socket destroys it, and
//! giving the power up loses all three.
//!
//! **Magnitude 1.** The percentage is of the effect's "magnitude 1" on
//! that power: the amount of the effect that is worth as much as the whole
//! power, by the same rates energy and recharge are reckoned with
//! ([`powers::value`]). So a 20% hold augment is as good on a small bolt as
//! on a capstone, and nobody keeps a table of powers times effects. A new
//! behaviour never comes out too small to see ([`Effect::floor`]). Powers
//! that are always on or switched on (passives, toggles, travel) have no
//! worth per use: their effects have fixed reference amounts
//! ([`Effect::reference`]).
//!
//! Augments are free: energy and recharge follow the power as it is
//! without them ([`powers::energy`], [`powers::recharge`]), except for the
//! two augments made for that ([`Effect::Quick`], [`Effect::Thrifty`]).
//!
//! Strength comes from the level of the villain that dropped it and from
//! its [`Grade`] ([`strength`]); what drops is rolled in [`roll`].

use crate::powers::{self, travel, Delivery, Fields, Lands, PowerDef};
use hw_net::{Augment, PowerId, SetAugment, Sheet, MAX_BAG, SOCKETS};

/// What an augment adds to. The numbers are saved in hero files: never
/// reuse one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Effect {
    // ---- Powers that are fired ----
    Damage = 1,
    Burn = 2,
    Drain = 3,
    Exploit = 4,
    Leap = 5,
    Knockdown = 6,
    Launch = 7,
    Push = 8,
    Hold = 9,
    Freeze = 10,
    Slow = 11,
    Confuse = 12,
    Fear = 13,
    Charm = 14,
    Disarm = 15,
    Weaken = 16,
    Range = 17,
    Area = 18,
    Wreck = 19,
    Spare = 20,
    Heal = 21,
    Shield = 22,
    Reflect = 23,
    Lasting = 24,
    Quick = 25,
    Thrifty = 26,
    // ---- Powers that are always on, or switched on ----
    Resist = 30,
    Dodge = 31,
    Brace = 32,
    Regen = 33,
    Might = 35,
    Haste = 36,
    Aura = 37,
    Ward = 38,
    Light = 39,
    // ---- Travel powers ----
    Speed = 40,
    Height = 41,
    Stomp = 42,
    Trail = 43,
}

impl Effect {
    pub const ALL: [Effect; 39] = [
        Effect::Damage,
        Effect::Burn,
        Effect::Drain,
        Effect::Exploit,
        Effect::Leap,
        Effect::Knockdown,
        Effect::Launch,
        Effect::Push,
        Effect::Hold,
        Effect::Freeze,
        Effect::Slow,
        Effect::Confuse,
        Effect::Fear,
        Effect::Charm,
        Effect::Disarm,
        Effect::Weaken,
        Effect::Range,
        Effect::Area,
        Effect::Wreck,
        Effect::Spare,
        Effect::Heal,
        Effect::Shield,
        Effect::Reflect,
        Effect::Lasting,
        Effect::Quick,
        Effect::Thrifty,
        Effect::Resist,
        Effect::Dodge,
        Effect::Brace,
        Effect::Regen,
        Effect::Might,
        Effect::Haste,
        Effect::Aura,
        Effect::Ward,
        Effect::Light,
        Effect::Speed,
        Effect::Height,
        Effect::Stomp,
        Effect::Trail,
    ];

    pub fn code(self) -> u8 {
        self as u8
    }

    pub fn from_code(c: u8) -> Option<Effect> {
        Self::ALL.into_iter().find(|e| e.code() == c)
    }

    pub fn of(a: Augment) -> Option<Effect> {
        Self::from_code(a.effect)
    }

    pub fn name(self) -> &'static str {
        match self {
            Effect::Damage => "Damage",
            Effect::Burn => "Burn",
            Effect::Drain => "Drain",
            Effect::Exploit => "Exploit",
            Effect::Leap => "Leap",
            Effect::Knockdown => "Knockdown",
            Effect::Launch => "Launch",
            Effect::Push => "Push",
            Effect::Hold => "Hold",
            Effect::Freeze => "Freeze",
            Effect::Slow => "Slow",
            Effect::Confuse => "Confuse",
            Effect::Fear => "Fear",
            Effect::Charm => "Charm",
            Effect::Disarm => "Disarm",
            Effect::Weaken => "Weaken",
            Effect::Range => "Range",
            Effect::Area => "Area",
            Effect::Wreck => "Wreck",
            Effect::Spare => "Spare",
            Effect::Heal => "Heal",
            Effect::Shield => "Shield",
            Effect::Reflect => "Reflect",
            Effect::Lasting => "Lasting",
            Effect::Quick => "Quick",
            Effect::Thrifty => "Thrifty",
            Effect::Resist => "Resist",
            Effect::Dodge => "Dodge",
            Effect::Brace => "Brace",
            Effect::Regen => "Regen",
            Effect::Might => "Might",
            Effect::Haste => "Haste",
            Effect::Aura => "Aura",
            Effect::Ward => "Ward",
            Effect::Light => "Light",
            Effect::Speed => "Speed",
            Effect::Height => "Height",
            Effect::Stomp => "Stomp",
            Effect::Trail => "Trail",
        }
    }

    /// What it does, in a few words (the bag's hover text).
    pub fn blurb(self) -> &'static str {
        match self {
            Effect::Damage => "hits harder",
            Effect::Burn => "the hit burns on",
            Effect::Drain => "some of the hit comes back as health",
            Effect::Exploit => "hits the helpless harder",
            Effect::Leap => "the hit leaps on to villains near",
            Effect::Knockdown => "staggers villains off their feet",
            Effect::Launch => "throws villains into the air",
            Effect::Push => "shoves villains away",
            Effect::Hold => "binds villains in place",
            Effect::Freeze => "freezes villains solid",
            Effect::Slow => "slows villains down",
            Effect::Confuse => "leaves villains confused",
            Effect::Fear => "sends villains running",
            Effect::Charm => "turns a villain to your side",
            Effect::Disarm => "stops villains attacking",
            Effect::Weaken => "villains hit take more from everyone",
            Effect::Range => "reaches farther",
            Effect::Area => "covers a wider area",
            Effect::Wreck => "tears up the street",
            Effect::Spare => "spares the street and bystanders",
            Effect::Heal => "heals more",
            Effect::Shield => "the shield stops more",
            Effect::Reflect => "sends blows back",
            Effect::Lasting => "lasts longer",
            Effect::Quick => "recharges sooner",
            Effect::Thrifty => "takes less energy",
            Effect::Resist => "you take less",
            Effect::Dodge => "blows miss you",
            Effect::Brace => "you take less standing still",
            Effect::Regen => "health comes back",
            Effect::Might => "everything you do hits harder",
            Effect::Haste => "everything recharges sooner",
            Effect::Aura => "friends near hit harder",
            Effect::Ward => "friends near take less",
            Effect::Light => "takes less of your energy recovery",
            Effect::Speed => "faster",
            Effect::Height => "jumps higher",
            Effect::Stomp => "lands harder",
            Effect::Trail => "hurts villains you pass",
        }
    }

    /// For a power that is always on or switched on: the amount this
    /// effect's magnitude 1 is, when the power has none of it itself.
    pub fn reference(self) -> f32 {
        match self {
            Effect::Resist => 0.25,
            Effect::Dodge => 0.2,
            Effect::Brace => 0.3,
            Effect::Regen => 0.01,
            Effect::Reflect => 0.2,
            Effect::Might => 0.25,
            Effect::Exploit => 0.25,
            Effect::Haste => 0.25,
            Effect::Aura => 0.2,
            Effect::Ward => 0.2,
            Effect::Speed => 0.25,
            Effect::Height => 0.25,
            Effect::Stomp => 1.0,
            Effect::Trail => 10.0,
            _ => 0.0,
        }
    }

    /// The least a new behaviour gives (seconds, metres, a share): less
    /// could not be seen in a fight.
    pub fn floor(self) -> f32 {
        match self {
            Effect::Hold | Effect::Freeze | Effect::Confuse => 0.8,
            Effect::Fear | Effect::Charm | Effect::Disarm => 1.0,
            Effect::Slow => 0.15,
            Effect::Weaken => 0.05,
            Effect::Push => 1.5,
            Effect::Launch => 4.0,
            _ => 0.0,
        }
    }
}

/// Seconds a slow from an augment lasts on a power that slows nobody
/// itself, and seconds a weakening does.
pub const SLOW_SECONDS: f64 = 4.0;
pub const WEAKEN_SECONDS: f64 = 6.0;
/// Villains a leap from an augment reaches on a power that leaps to none.
pub const LEAP_TO: u8 = 2;
/// How much of a full magnitude a few effects give, where the plain rate
/// would be too much to fight with (a bolt that shoves villains 30 m).
const DRAIN_TUNE: f32 = 0.5;
const LEAP_TUNE: f32 = 1.5;
const PUSH_TUNE: f32 = 0.4;
const LAUNCH_TUNE: f32 = 0.5;
const WEAKEN_TUNE: f32 = 2.0;
const SPARE_TUNE: f32 = 2.0;
/// Street damage a wreck augment's magnitude 1 is on a power with none.
const WRECK_REFERENCE: f32 = 2.0;
/// A slow never stops a villain outright, a shield never stops everything.
const MOST_SLOW: f32 = 0.9;
const MOST_SHIELD: f32 = 0.9;

/// Is the power always on or switched on (not fired)?
fn sustained(d: &PowerDef) -> bool {
    d.passive() || d.toggle
}

/// Does `effect` fit power `d`? (A charm cannot go on a shield.)
pub fn fits(effect: Effect, d: &PowerDef) -> bool {
    let b = &d.fields;
    let fired = !sustained(d);
    let delivery = d.delivery();
    let harms = fired && b.damage > 0.0;
    let foes = fired
        && (matches!(
            delivery,
            Delivery::Melee | Delivery::Ranged | Delivery::Line | Delivery::Cone | Delivery::Sky
        ) || harms);
    let single = fired && matches!(delivery, Delivery::Melee | Delivery::Ranged);
    let travels = b.travel != 0;
    let kept = sustained(d) && !travels;
    // What a kept power is for. One that is neither takes both kinds.
    let defends = b.resist > 0.0
        || b.dodge > 0.0
        || b.brace > 0.0
        || b.regen > 0.0
        || b.reflect > 0.0
        || b.steadfast > 0.0
        || b.angel > 0.0
        || b.ward > 0.0
        || b.second_wind > 0.0
        || b.get_up > 0.0;
    let attacks = b.might > 0.0
        || b.haste > 0.0
        || b.aura > 0.0
        || b.exploit > 0.0
        || b.ambush > 0.0
        || b.chain > 0.0
        || b.cuff > 0.0
        || b.bounty > 0.0;
    let defensive = kept && (defends || !attacks);
    let offensive = kept && (attacks || !defends);
    match effect {
        Effect::Damage | Effect::Burn | Effect::Drain => harms,
        // A kept power for attack finds weak spots too.
        Effect::Exploit => harms || offensive,
        // What floors every time cannot floor more.
        Effect::Knockdown => harms && b.knock < 1.0,
        Effect::Push => harms,
        Effect::Leap => harms && single,
        Effect::Launch => harms && matches!(delivery, Delivery::Melee | Delivery::Burst),
        Effect::Hold
        | Effect::Freeze
        | Effect::Slow
        | Effect::Confuse
        | Effect::Fear
        | Effect::Disarm
        | Effect::Weaken => foes,
        Effect::Charm => single,
        Effect::Range => {
            fired
                && b.range > 0.0
                && !matches!(delivery, Delivery::Summon | Delivery::Rescue)
                && !matches!(b.lands, Lands::Home | Lands::GateMark | Lands::GateOpen)
        }
        Effect::Area => fired && b.radius > 0.0,
        // Only where the street takes it: beams, bursts, strikes from the
        // sky and leaps.
        Effect::Wreck => {
            harms
                && matches!(
                    delivery,
                    Delivery::Line | Delivery::Burst | Delivery::Sky | Delivery::Blink
                )
        }
        Effect::Spare => fired && b.wreck > 0.0,
        Effect::Heal => fired && b.heal > 0.0,
        Effect::Shield => fired && b.shield > 0.0 && b.shield < MOST_SHIELD,
        Effect::Reflect => (fired && b.shield > 0.0) || defensive,
        Effect::Lasting => fired && b.lasts > 0.0,
        Effect::Quick => fired && !powers::is_free(d.id) && powers::recharge(d) > 1.0,
        Effect::Thrifty => fired && powers::energy(d) > 0.0,
        Effect::Resist | Effect::Dodge | Effect::Brace | Effect::Regen => defensive,
        // A helper called can be a stronger one.
        Effect::Might => offensive || (fired && b.summons > 0),
        Effect::Haste => offensive,
        Effect::Aura => kept && b.aura > 0.0,
        Effect::Ward => kept && b.ward > 0.0,
        Effect::Light => d.toggle,
        Effect::Speed => travels,
        Effect::Height | Effect::Stomp => b.travel & travel::JUMP != 0,
        Effect::Trail => b.travel & (travel::SPEED | travel::SPRINT) != 0,
    }
}

/// Every effect that fits `d`.
pub fn fitting(d: &PowerDef) -> impl Iterator<Item = Effect> + '_ {
    Effect::ALL.into_iter().filter(move |&e| fits(e, d))
}

/// Sockets power `d` has: three, or as many as there are effects that fit
/// it (Flight only takes speed, so it has one).
pub fn sockets_of(d: &PowerDef) -> usize {
    fitting(d).count().min(SOCKETS)
}

/// Power `d` as it is with `augments` in its sockets: its fields with
/// every augment's share added. Each share is reckoned from the power as
/// it is without augments, so the order of the sockets does not matter.
/// Augments that do not fit, and a second one of the same effect, add
/// nothing.
pub fn apply(d: &PowerDef, augments: &[Augment]) -> Fields {
    let mut f = d.fields;
    let mut seen = [None; SOCKETS];
    for (i, a) in augments.iter().enumerate().take(SOCKETS) {
        let Some(e) = Effect::of(*a).filter(|_| !a.is_none()) else {
            continue;
        };
        if !fits(e, d) || seen.contains(&Some(e)) {
            continue;
        }
        seen[i] = Some(e);
        add(&mut f, d, e, a.percent as f32 / 100.0);
    }
    f
}

/// Adds share `p` of effect `e`'s magnitude 1 on power `d` to `f`.
fn add(f: &mut Fields, d: &PowerDef, e: Effect, p: f32) {
    let b = &d.fields;
    // What the power is worth on each villain it reaches, each time it
    // lands: magnitude 1 of anything done to a villain is worth this.
    let each = powers::value(d) / (powers::targets(d) * b.repeats.max(1) as f32).max(1.0);
    // A new behaviour gets at least its floor; more of an old one just adds.
    let fresh = |old: f32, add: f32| {
        if old == 0.0 {
            add.max(e.floor())
        } else {
            add
        }
    };
    // For kept powers: a share of the power's own amount, or of the
    // effect's reference where it has none.
    let kept = |old: f32| p * if old > 0.0 { old } else { e.reference() };
    let per_damage = if b.damage > 0.0 { each / b.damage } else { 0.0 };
    match e {
        Effect::Damage => f.damage += p * each,
        Effect::Burn => f.dot += p * per_damage,
        Effect::Drain => f.drain += p * per_damage * DRAIN_TUNE,
        Effect::Exploit => {
            f.exploit += if b.damage > 0.0 {
                p * per_damage * 2.0
            } else {
                kept(b.exploit)
            }
        }
        Effect::Leap => {
            let to = if b.spread_to > 0 {
                b.spread_to
            } else {
                LEAP_TO
            };
            f.spread_to = to;
            f.spread += p * LEAP_TUNE / to as f32;
        }
        Effect::Knockdown => f.knock = (f.knock + p).min(1.0),
        Effect::Launch => f.launch += fresh(b.launch, p * each * LAUNCH_TUNE),
        Effect::Push => {
            let more = fresh(b.push, p * each * PUSH_TUNE);
            // A power that pulls pulls harder.
            f.push += if b.push < 0.0 { -more } else { more };
        }
        Effect::Hold => f.hold += fresh(b.hold as f32, p * each / 8.0) as f64,
        Effect::Freeze => f.freeze += fresh(b.freeze as f32, p * each / 5.0) as f64,
        Effect::Confuse => f.confuse += fresh(b.confuse as f32, p * each / 8.0) as f64,
        Effect::Fear => f.fear += fresh(b.fear as f32, p * each / 7.0) as f64,
        Effect::Charm => f.charm += fresh(b.charm as f32, p * each / 9.0) as f64,
        Effect::Disarm => f.disarm += fresh(b.disarm as f32, p * each / 5.0) as f64,
        Effect::Slow => {
            let secs = if b.slow > 0.0 && b.slow_secs > 0.0 {
                b.slow_secs
            } else {
                SLOW_SECONDS
            };
            f.slow_secs = secs;
            f.slow = (f.slow + fresh(b.slow, p * each / (secs as f32 * 4.0))).min(MOST_SLOW);
        }
        Effect::Weaken => {
            let secs = if b.lasts > 0.0 {
                b.lasts
            } else {
                WEAKEN_SECONDS
            };
            f.lasts = secs;
            f.weaken += fresh(b.weaken, p * each * WEAKEN_TUNE / (secs as f32 * 20.0));
        }
        Effect::Range => f.range += p as f64 * b.range,
        Effect::Area => f.radius += p as f64 * b.radius,
        Effect::Wreck => {
            f.wreck += p * if b.wreck > 0.0 {
                b.wreck
            } else {
                WRECK_REFERENCE
            }
        }
        Effect::Spare => f.spare = (f.spare + p * SPARE_TUNE).min(1.0),
        Effect::Heal => f.heal += p * b.heal,
        Effect::Shield => f.shield = (f.shield + p * b.shield).min(MOST_SHIELD.max(b.shield)),
        Effect::Reflect => f.reflect += kept(b.reflect),
        Effect::Lasting => f.lasts += p as f64 * b.lasts,
        Effect::Quick => f.quick += p,
        Effect::Thrifty => f.thrifty += p,
        Effect::Resist => f.resist = 1.0 - (1.0 - f.resist) * (1.0 - kept(b.resist)),
        Effect::Dodge => f.dodge = 1.0 - (1.0 - f.dodge) * (1.0 - kept(b.dodge)),
        Effect::Brace => f.brace = 1.0 - (1.0 - f.brace) * (1.0 - kept(b.brace)),
        Effect::Regen => f.regen += kept(b.regen),
        Effect::Might if b.summons > 0 => f.summon_power += p * b.summon_power,
        Effect::Might => f.might += kept(b.might),
        Effect::Haste => f.haste += kept(b.haste),
        Effect::Aura => f.aura += kept(b.aura),
        Effect::Ward => f.ward += kept(b.ward),
        Effect::Light => f.upkeep *= 1.0 - p.min(1.0),
        Effect::Speed => {
            // On the ground or in the air, by what the power is.
            if b.travel & travel::FLY != 0 {
                f.soar += kept(0.0);
            } else if b.travel & (travel::JUMP | travel::GLIDE) != 0 {
                // A super jump's speed through the air, and a glide's.
                f.leap += kept(0.0);
            } else {
                f.swift += kept(0.0);
            }
        }
        Effect::Height => f.height += kept(0.0),
        Effect::Stomp => f.stomp += kept(0.0),
        Effect::Trail => f.trail += kept(b.trail),
    }
}

/// What augment `a` does on power `d` for a hero of `level`, in a few
/// words ("also holds for 0.9 s"); `None` where it does nothing there.
pub fn describe(d: &PowerDef, a: Augment, level: u8) -> Option<String> {
    let e = Effect::of(a).filter(|&e| !a.is_none() && fits(e, d))?;
    let b = d.fields;
    let f = apply(d, &[a]);
    let scale = crate::combat::level_scale(level);
    let pct = |x: f32| format!("{:.0}%", x * 100.0);
    let secs = |new: f64, old: f64| {
        if old > 0.0 {
            format!("{:.1} s (was {:.1})", new, old)
        } else {
            format!("for {:.1} s", new)
        }
    };
    Some(match e {
        Effect::Damage => format!(
            "{:.0} damage (was {:.0})",
            f.damage * scale,
            b.damage * scale
        ),
        Effect::Burn => format!("burns for {:.0} more", f.damage * f.dot * scale),
        Effect::Drain => format!("{} of the hit comes back as health", pct(f.drain)),
        Effect::Exploit => format!("{} more on the helpless", pct(f.exploit)),
        Effect::Leap => format!("{} of the hit leaps to {} more", pct(f.spread), f.spread_to),
        Effect::Knockdown => {
            let hits = |floor: f32| (floor / f.knock.max(0.01)).ceil().max(1.0) as u32;
            let [minion, _, boss] = crate::combat::STAGGER_FLOORS;
            if f.knock >= 1.0 {
                "floors anyone at one hit".into()
            } else {
                format!(
                    "floors a minion in {} hits, a boss in {}",
                    hits(minion),
                    hits(boss)
                )
            }
        }
        Effect::Launch => format!("throws them up at {:.0} m/s", f.launch),
        Effect::Push => format!("shoves them {:.1} m", f.push.abs()),
        Effect::Hold => format!("holds {}", secs(f.hold, b.hold)),
        Effect::Freeze => format!("freezes {}", secs(f.freeze, b.freeze)),
        Effect::Confuse => format!("confuses {}", secs(f.confuse, b.confuse)),
        Effect::Fear => format!("frightens {}", secs(f.fear, b.fear)),
        Effect::Charm => format!("charms {}", secs(f.charm, b.charm)),
        Effect::Disarm => format!("disarms {}", secs(f.disarm, b.disarm)),
        Effect::Slow => format!("slows {} for {:.0} s", pct(f.slow), f.slow_secs),
        Effect::Weaken => format!("they take {} more for {:.0} s", pct(f.weaken), f.lasts),
        Effect::Range => format!("reaches {:.0} m (was {:.0})", f.range, b.range),
        Effect::Area => format!("{} wider", pct(a.percent as f32 / 100.0)),
        Effect::Wreck => format!("wrecks the street {:.1} (was {:.1})", f.wreck, b.wreck),
        Effect::Spare => format!("spares {} of the street", pct(f.spare)),
        Effect::Heal => format!("heals {:.0} (was {:.0})", f.heal * scale, b.heal * scale),
        Effect::Shield => format!("stops {} (was {})", pct(f.shield), pct(b.shield)),
        Effect::Reflect => format!("sends back {} of a blow", pct(f.reflect)),
        Effect::Lasting => format!("lasts {:.0} s (was {:.0})", f.lasts, b.lasts),
        Effect::Quick => format!(
            "ready in {:.1} s (was {:.1})",
            recharge(d, &f),
            powers::recharge(d)
        ),
        Effect::Thrifty => format!(
            "takes {:.0} energy (was {:.0})",
            energy(d, &f),
            powers::energy(d)
        ),
        Effect::Resist => format!("you take {} less", pct(f.resist)),
        Effect::Dodge => format!("{} of blows miss you", pct(f.dodge)),
        Effect::Brace => format!("{} less standing still", pct(f.brace)),
        Effect::Regen => format!("{:.1}% of health a second", f.regen * 100.0),
        Effect::Might if b.summons > 0 => format!(
            "the helper is {} stronger",
            pct(f.summon_power / b.summon_power.max(0.01) - 1.0)
        ),
        Effect::Might => format!("you hit {} harder", pct(f.might)),
        Effect::Haste => format!("powers recharge {} sooner", pct(f.haste)),
        Effect::Aura => format!("friends hit {} harder", pct(f.aura)),
        Effect::Ward => format!("friends take {} less", pct(f.ward)),
        Effect::Light => format!(
            "takes {} of energy recovery (was {})",
            pct(f.upkeep),
            pct(b.upkeep)
        ),
        Effect::Speed => format!("{} faster", pct(f.swift + f.soar + f.leap)),
        Effect::Height => format!("jumps {} higher", pct(f.height)),
        Effect::Stomp => format!("lands {} harder", pct(f.stomp)),
        Effect::Trail => format!(
            "{:.0} damage a second to villains you pass",
            f.trail * scale
        ),
    })
}

/// Energy power `d` takes with fields `f` (its own with augments added).
pub fn energy(d: &PowerDef, f: &Fields) -> f32 {
    let base = powers::energy(d);
    if base <= 0.0 {
        return 0.0;
    }
    (base * (1.0 - 0.5 * f.thrifty.clamp(0.0, 1.0)))
        .round()
        .max(1.0)
}

/// Seconds before power `d` can be used again with fields `f`.
pub fn recharge(d: &PowerDef, f: &Fields) -> f64 {
    let base = powers::recharge(d);
    ((base * (1.0 - 0.5 * f.quick.clamp(0.0, 1.0) as f64)) * 10.0).round() / 10.0
}

// ---- Grades and strength ----

/// How rare an augment is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Grade {
    Common,
    Rare,
    Relic,
}

impl Grade {
    pub const ALL: [Grade; 3] = [Grade::Common, Grade::Rare, Grade::Relic];

    pub fn code(self) -> u8 {
        self as u8
    }

    pub fn from_code(c: u8) -> Grade {
        match c {
            1 => Grade::Rare,
            2 => Grade::Relic,
            _ => Grade::Common,
        }
    }

    pub fn of(a: Augment) -> Grade {
        Self::from_code(a.grade)
    }

    pub fn name(self) -> &'static str {
        match self {
            Grade::Common => "Common",
            Grade::Rare => "Rare",
            Grade::Relic => "Relic",
        }
    }

    /// How much stronger than a common one of the same level.
    pub fn times(self) -> f32 {
        match self {
            Grade::Common => 1.0,
            Grade::Rare => 1.25,
            Grade::Relic => 1.5,
        }
    }

    /// How many commons one is worth when traded in.
    pub fn worth(self) -> f32 {
        match self {
            Grade::Common => 1.0,
            Grade::Rare => 3.0,
            Grade::Relic => 10.0,
        }
    }
}

/// A common augment's percent from a level 1 villain and from a level 20 one.
pub const COMMON_PERCENT: (f32, f32) = (8.0, 20.0);

/// The percent of an augment of `grade` dropped by a villain of `level`:
/// commons 8% to 20% over the levels, rares a quarter more, relics half.
pub fn strength(grade: Grade, level: u8) -> u8 {
    let t = (level.clamp(1, crate::progress::MAX_LEVEL) - 1) as f32
        / (crate::progress::MAX_LEVEL - 1) as f32;
    let common = COMMON_PERCENT.0 + (COMMON_PERCENT.1 - COMMON_PERCENT.0) * t;
    (common * grade.times()).round() as u8
}

/// The strongest augment there can be (a relic from a level 20 villain).
pub fn most() -> u8 {
    strength(Grade::Relic, crate::progress::MAX_LEVEL)
}

/// An augment of `effect` and `grade` as a villain of `level` drops it.
pub fn make(effect: Effect, grade: Grade, level: u8) -> Augment {
    Augment {
        effect: effect.code(),
        percent: strength(grade, level),
        grade: grade.code(),
    }
}

// ---- Setting, replacing, trading in ----

/// Why an augment cannot be set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetError {
    /// Nothing at that place in the bag.
    NoSuchAugment,
    /// The hero does not own the power.
    NotOwned,
    NoSuchSocket,
    /// The effect does nothing on that power.
    DoesNotFit,
    /// Another socket of the power holds the same effect.
    SameEffect,
}

impl std::fmt::Display for SetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SetError::NoSuchAugment => write!(f, "no such augment"),
            SetError::NotOwned => write!(f, "you do not have that power"),
            SetError::NoSuchSocket => write!(f, "no such socket"),
            SetError::DoesNotFit => write!(f, "it does nothing on this power"),
            SetError::SameEffect => write!(f, "this power has that effect in another socket"),
        }
    }
}

/// Can augment `a` go into socket `slot` of `power`? (What is in that
/// socket now does not matter: it would be replaced.)
pub fn can_set_augment(
    sheet: &Sheet,
    a: Augment,
    power: PowerId,
    slot: u8,
) -> Result<(), SetError> {
    let e = Effect::of(a)
        .filter(|_| !a.is_none())
        .ok_or(SetError::NoSuchAugment)?;
    let d = powers::get(power)
        .filter(|_| sheet.has(power))
        .ok_or(SetError::NotOwned)?;
    if slot as usize >= sockets_of(d) {
        return Err(SetError::NoSuchSocket);
    }
    if !fits(e, d) {
        return Err(SetError::DoesNotFit);
    }
    let others = sheet
        .augments
        .iter()
        .filter(|s| s.power == power && s.slot != slot);
    if others.into_iter().any(|s| s.augment.effect == a.effect) {
        return Err(SetError::SameEffect);
    }
    Ok(())
}

/// Can the augment at `bag` go into socket `slot` of `power`?
pub fn can_set(sheet: &Sheet, bag: usize, power: PowerId, slot: u8) -> Result<(), SetError> {
    let a = *sheet.bag.get(bag).ok_or(SetError::NoSuchAugment)?;
    can_set_augment(sheet, a, power, slot)
}

/// Sets the augment at `bag` into socket `slot` of `power`, for good.
/// Returns the augment that was there and is now lost, if any.
pub fn set(
    sheet: &mut Sheet,
    bag: usize,
    power: PowerId,
    slot: u8,
) -> Result<Option<Augment>, SetError> {
    can_set(sheet, bag, power, slot)?;
    let augment = sheet.bag.remove(bag);
    let old = sheet
        .augments
        .iter()
        .position(|s| s.power == power && s.slot == slot)
        .map(|i| sheet.augments.remove(i).augment);
    sheet.augments.push(SetAugment {
        power,
        slot,
        augment,
    });
    Ok(old)
}

/// Karma a common augment from a level 1 villain trades in for. About
/// twenty commons of a hero's level buy that hero a first-row power.
pub const TRADE_KARMA: f32 = crate::progress::ROW1_PRICE as f32 / 20.0;

/// Karma augment `a` trades in for: by the level it came from (its
/// percent) and its grade. The karma buys powers; it does not raise the
/// level.
pub fn trade_value(a: Augment) -> u32 {
    let grade = Grade::of(a);
    let common = a.percent as f32 / grade.times();
    (TRADE_KARMA * common / COMMON_PERCENT.0 * grade.worth()).round() as u32
}

/// Trades the augment at `bag` in. Returns the karma it gave.
pub fn trade(sheet: &mut Sheet, bag: usize) -> Result<u32, SetError> {
    if bag >= sheet.bag.len() {
        return Err(SetError::NoSuchAugment);
    }
    let karma = trade_value(sheet.bag.remove(bag));
    sheet.karma = sheet.karma.saturating_add(karma);
    Ok(karma)
}

/// Puts a found augment into the hero's bag. With the bag full it is
/// traded in at once: returns the karma it gave then.
pub fn collect(sheet: &mut Sheet, a: Augment) -> Option<u32> {
    if sheet.bag.len() < MAX_BAG {
        sheet.bag.push(a);
        return None;
    }
    let karma = trade_value(a);
    sheet.karma = sheet.karma.saturating_add(karma);
    Some(karma)
}

/// Loses the augments of a power given up.
pub fn lose_with(sheet: &mut Sheet, power: PowerId) {
    sheet.augments.retain(|s| s.power != power);
}

/// Makes the augments of a hero file sane: known effects no stronger than
/// any drop can be, set only into powers the hero owns, where they fit,
/// one to a socket and no effect twice on a power; a bag no bigger than
/// it can be.
pub fn sanitize(s: &mut Sheet) {
    let most = most();
    let sane =
        |a: &Augment| Effect::of(*a).is_some() && !a.is_none() && a.percent <= most && a.grade <= 2;
    let mut kept: Vec<SetAugment> = Vec::new();
    for a in &s.augments {
        let Some(d) = powers::get(a.power).filter(|_| s.has(a.power)) else {
            continue;
        };
        let (Some(e), true) = (Effect::of(a.augment), sane(&a.augment)) else {
            continue;
        };
        let taken = kept.iter().any(|k| {
            k.power == a.power && (k.slot == a.slot || k.augment.effect == a.augment.effect)
        });
        if (a.slot as usize) < sockets_of(d) && fits(e, d) && !taken {
            kept.push(*a);
        }
    }
    s.augments = kept;
    s.bag.retain(sane);
    s.bag.truncate(MAX_BAG);
}

// ---- What drops ----

/// Drops a hero finds in an hour of fighting, about (the chances below
/// are set for it; `tests/augments.rs` plays it).
pub const DROPS_AN_HOUR: f64 = 20.0;

/// The chance that a beaten villain of each rank (minion, lieutenant,
/// boss) drops an augment for a hero who fought it.
pub const DROP_CHANCE: [f64; 3] = [0.023, 0.2, 1.0];

/// The chance of each grade (common, rare, relic) from each rank.
pub const GRADE_CHANCE: [[f64; 3]; 3] = [[0.9, 0.09, 0.01], [0.65, 0.3, 0.05], [0.3, 0.45, 0.25]];

/// The chance of each grade from a find (the thing a hideout bust ends
/// at): never a common one.
pub const FIND_GRADE_CHANCE: [f64; 3] = [0.0, 0.6, 0.4];

/// Of ten drops, this many fit a power the hero owns.
pub const FITS_OWNED: f64 = 0.7;

/// Numbers in 0..1 from a seed, the same every run (splitmix64).
pub struct Dice(pub u64);

impl Dice {
    /// Dice for one drop: by the tick, the villain and the hero.
    pub fn for_drop(tick: u64, villain: u32, hero: u32) -> Self {
        Dice(
            tick.wrapping_mul(0x9E37_79B9_7F4A_7C15)
                ^ (villain as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9)
                ^ (hero as u64).wrapping_mul(0x94D0_49BB_1331_11EB),
        )
    }

    pub fn roll(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut x = self.0;
        x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        x ^= x >> 31;
        (x >> 11) as f64 / (1u64 << 53) as f64
    }

    fn pick<T: Copy>(&mut self, from: &[T]) -> Option<T> {
        if from.is_empty() {
            return None;
        }
        let i = (self.roll() * from.len() as f64) as usize;
        Some(from[i.min(from.len() - 1)])
    }

    fn grade(&mut self, chances: [f64; 3]) -> Grade {
        let x = self.roll() * chances.iter().sum::<f64>();
        if x < chances[0] {
            Grade::Common
        } else if x < chances[0] + chances[1] {
            Grade::Rare
        } else {
            Grade::Relic
        }
    }
}

/// The effects that fit at least one power on `sheet`.
pub fn useful(sheet: &Sheet) -> Vec<Effect> {
    let owned: Vec<&PowerDef> = powers::POWERS.iter().filter(|d| sheet.has(d.id)).collect();
    Effect::ALL
        .into_iter()
        .filter(|&e| owned.iter().any(|d| fits(e, d)))
        .collect()
}

/// Which effect a drop for the hero of `sheet` has: seven in ten fit a
/// power the hero owns, the rest are any effect there is.
fn effect_for(sheet: &Sheet, dice: &mut Dice) -> Effect {
    let fitting = if dice.roll() < FITS_OWNED {
        dice.pick(&useful(sheet))
    } else {
        None
    };
    fitting
        .or_else(|| dice.pick(&Effect::ALL))
        .unwrap_or(Effect::Damage)
}

/// What a villain of `rank` (0 minion, 1 lieutenant, 2 boss) and `level`
/// carries from the moment it appears, if anything: how good it is and
/// how strong. Which effect it has is each hero's own (see [`drop_for`]),
/// so `effect` is 0 here. Everyone can see a villain carries one (it
/// glints in its grade's colour), and the villain is that much tougher.
pub fn carry(rank: usize, level: u8, dice: &mut Dice) -> Option<Augment> {
    let rank = rank.min(2);
    if dice.roll() >= DROP_CHANCE[rank] {
        return None;
    }
    let grade = dice.grade(GRADE_CHANCE[rank]);
    Some(Augment {
        effect: 0,
        percent: strength(grade, level),
        grade: grade.code(),
    })
}

/// The augment the hero of `sheet` gets from a beaten villain that
/// carried `carried`: its grade and strength, with an effect of the
/// hero's own (seven in ten fit a power they have).
pub fn drop_for(sheet: &Sheet, carried: Augment, dice: &mut Dice) -> Augment {
    Augment {
        effect: effect_for(sheet, dice).code(),
        ..carried
    }
}

/// Both in one: what a beaten villain of `rank` and `level` drops for the
/// hero of `sheet`, if anything (tools and tests).
pub fn roll(sheet: &Sheet, rank: usize, level: u8, dice: &mut Dice) -> Option<Augment> {
    carry(rank, level, dice).map(|c| drop_for(sheet, c, dice))
}

/// What a find of `level` holds for the hero of `sheet` (the thing a
/// hideout bust ends at): always something, never a common one.
pub fn find(sheet: &Sheet, level: u8, dice: &mut Dice) -> Augment {
    let grade = dice.grade(FIND_GRADE_CHANCE);
    make(effect_for(sheet, dice), grade, level)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::powers::{def_of, POWERS};

    fn aug(e: Effect, percent: u8) -> Augment {
        Augment {
            effect: e.code(),
            percent,
            grade: 0,
        }
    }

    #[test]
    fn effect_numbers_are_unique_and_round_trip() {
        let mut seen = std::collections::BTreeSet::new();
        for e in Effect::ALL {
            assert!(seen.insert(e.code()), "{e:?}");
            assert!(e.code() != 0);
            assert_eq!(Effect::from_code(e.code()), Some(e));
        }
        assert_eq!(Effect::from_code(0), None);
    }

    #[test]
    fn no_augment_changes_nothing() {
        for d in POWERS {
            assert_eq!(apply(d, &[]), d.fields, "{}", d.name);
            assert_eq!(apply(d, &[Augment::NONE; SOCKETS]), d.fields, "{}", d.name);
            assert_eq!(energy(d, &d.fields), powers::energy(d), "{}", d.name);
            assert_eq!(recharge(d, &d.fields), powers::recharge(d), "{}", d.name);
        }
    }

    #[test]
    fn every_power_has_sockets() {
        let mut few = Vec::new();
        for d in POWERS {
            let n = sockets_of(d);
            // The portal's two buttons are the only powers nothing fits.
            assert!(
                n >= 1 || matches!(d.fields.lands, Lands::GateMark | Lands::GateOpen),
                "{}: nothing fits",
                d.name
            );
            // Travel powers take few effects (Flight only speed); what is
            // fired or kept on has all three sockets.
            if d.fields.travel == 0
                && !matches!(
                    d.fields.lands,
                    Lands::Home | Lands::GateMark | Lands::GateOpen
                )
                && n != SOCKETS
            {
                few.push(format!("{}: {:?}", d.name, fitting(d).collect::<Vec<_>>()));
            }
        }
        assert!(few.is_empty(), "too few effects fit: {few:#?}");
    }

    #[test]
    fn every_fitting_effect_does_something_and_says_so() {
        for d in POWERS {
            for e in fitting(d) {
                let a = aug(e, 20);
                assert_ne!(apply(d, &[a]), d.fields, "{} with {e:?}", d.name);
                let words = describe(d, a, 10);
                assert!(words.is_some_and(|w| !w.is_empty()), "{} {e:?}", d.name);
            }
            // And what does not fit does nothing.
            for e in Effect::ALL.into_iter().filter(|&e| !fits(e, d)) {
                assert_eq!(apply(d, &[aug(e, 20)]), d.fields, "{} {e:?}", d.name);
                assert_eq!(describe(d, aug(e, 20), 10), None);
            }
        }
    }

    #[test]
    fn a_hold_is_worth_the_same_share_on_a_bolt_and_on_a_capstone() {
        // Atomic Ray and a capstone attack: 20% of each one's worth, in
        // seconds of hold on each villain reached.
        for id in [10, 30] {
            let d = def_of(PowerId(id));
            let f = apply(d, &[aug(Effect::Hold, 20)]);
            let worth = powers::value(d);
            let added = (f.hold - d.fields.hold) as f32
                * 8.0
                * powers::targets(d)
                * d.fields.repeats.max(1) as f32;
            let share = added / worth;
            // The floor may lift a small power's share; never below 20%.
            assert!(share >= 0.199, "{}: {share}", d.name);
            if (f.hold - d.fields.hold) as f32 > Effect::Hold.floor() {
                assert!((share - 0.2).abs() < 0.01, "{}: {share}", d.name);
            }
        }
    }

    #[test]
    fn a_new_behaviour_is_never_too_small_to_see() {
        let strike = def_of(powers::STRIKE);
        let f = apply(strike, &[aug(Effect::Hold, 8)]);
        assert!(f.hold >= Effect::Hold.floor() as f64, "{}", f.hold);
        let f = apply(strike, &[aug(Effect::Fear, 8)]);
        assert!(f.fear >= Effect::Fear.floor() as f64);
    }

    #[test]
    fn the_same_effect_twice_counts_once_and_order_does_not_matter() {
        let d = def_of(PowerId(10));
        let (a, b, c) = (
            aug(Effect::Damage, 20),
            aug(Effect::Burn, 20),
            aug(Effect::Hold, 20),
        );
        assert_eq!(apply(d, &[a, a, a]), apply(d, &[a]));
        assert_eq!(apply(d, &[a, b, c]), apply(d, &[c, a, b]));
    }

    #[test]
    fn augments_do_not_change_what_a_power_costs_but_quick_and_thrifty_do() {
        let d = def_of(PowerId(10));
        let f = apply(d, &[aug(Effect::Damage, 30), aug(Effect::Hold, 30)]);
        assert_eq!(energy(d, &f), powers::energy(d));
        assert_eq!(recharge(d, &f), powers::recharge(d));
        let f = apply(d, &[aug(Effect::Quick, 30), aug(Effect::Thrifty, 30)]);
        assert!(energy(d, &f) < powers::energy(d));
        assert!(recharge(d, &f) < powers::recharge(d));
    }

    #[test]
    fn flight_takes_speed_and_nothing_that_touches_its_energy() {
        let flight = def_of(PowerId(5));
        let fit: Vec<Effect> = fitting(flight).collect();
        assert_eq!(fit, vec![Effect::Speed], "{fit:?}");
    }

    #[test]
    fn strength_goes_by_level_and_grade() {
        assert_eq!(strength(Grade::Common, 1), 8);
        assert_eq!(strength(Grade::Common, 20), 20);
        assert_eq!(strength(Grade::Rare, 1), 10);
        assert_eq!(strength(Grade::Rare, 20), 25);
        assert_eq!(strength(Grade::Relic, 1), 12);
        assert_eq!(strength(Grade::Relic, 20), 30);
        assert_eq!(strength(Grade::Common, 10), 14);
        assert_eq!(most(), 30);
    }

    fn hero() -> Sheet {
        let mut s = crate::progress::starter();
        s.karma = 0;
        s
    }

    #[test]
    fn setting_is_for_good_and_three_must_differ() {
        let mut s = hero();
        let ray = PowerId(10);
        s.bag = vec![
            aug(Effect::Damage, 8),
            aug(Effect::Damage, 14),
            aug(Effect::Burn, 8),
            aug(Effect::Heal, 8),
            aug(Effect::Speed, 8),
        ];
        assert_eq!(set(&mut s, 0, ray, 0), Ok(None));
        // The same effect cannot go into another socket of that power,
        assert_eq!(can_set(&s, 0, ray, 1), Err(SetError::SameEffect));
        // but a better one replaces it, and the old one is lost.
        assert_eq!(set(&mut s, 0, ray, 0), Ok(Some(aug(Effect::Damage, 8))));
        assert_eq!(s.sockets(ray)[0], aug(Effect::Damage, 14));
        assert_eq!(set(&mut s, 0, ray, 1), Ok(None));
        // What does nothing there is refused; so is a power not owned.
        assert_eq!(can_set(&s, 0, ray, 2), Err(SetError::DoesNotFit));
        assert_eq!(can_set(&s, 0, PowerId(12), 0), Err(SetError::NotOwned));
        assert_eq!(can_set(&s, 0, ray, 3), Err(SetError::NoSuchSocket));
        assert_eq!(can_set(&s, 9, ray, 2), Err(SetError::NoSuchAugment));
        // The free powers have sockets too.
        assert_eq!(set(&mut s, 1, PowerId(3), 0), Ok(None));
        assert_eq!(s.bag, vec![aug(Effect::Heal, 8)]);
        assert_eq!(s.augments.len(), 3);
    }

    #[test]
    fn trading_in_is_generous_and_does_not_raise_the_level() {
        let common = |level| make(Effect::Damage, Grade::Common, level);
        assert_eq!(trade_value(common(1)), 48);
        assert_eq!(trade_value(common(20)), 120);
        // A rare counts three commons of its level, a relic ten.
        assert_eq!(
            trade_value(make(Effect::Damage, Grade::Rare, 20)),
            trade_value(common(20)) * 3
        );
        assert_eq!(
            trade_value(make(Effect::Damage, Grade::Relic, 20)),
            trade_value(common(20)) * 10
        );
        // About twenty commons of a hero's level buy a first-row power.
        for level in [1u8, 10, 20] {
            let twenty = 20 * trade_value(common(level));
            let mut s = hero();
            s.powers.clear();
            // A hero of that level owns about a power every level and a half.
            for _ in 0..(level as usize * 2 / 3) {
                s.powers.push(hw_net::OwnedPower {
                    power: PowerId(0),
                    paid: 0,
                });
            }
            let price = crate::progress::price(&s, PowerId(10));
            let ratio = twenty as f32 / price as f32;
            assert!((0.25..=1.5).contains(&ratio), "level {level}: {ratio}");
        }
        let mut s = hero();
        s.bag = vec![common(10)];
        let before = s.lifetime;
        assert_eq!(trade(&mut s, 0), Ok(84));
        assert_eq!((s.karma, s.lifetime, s.bag.len()), (84, before, 0));
        assert_eq!(trade(&mut s, 0), Err(SetError::NoSuchAugment));
    }

    #[test]
    fn a_full_bag_trades_the_next_find_in_at_once() {
        let mut s = hero();
        let a = make(Effect::Damage, Grade::Common, 1);
        for _ in 0..MAX_BAG {
            assert_eq!(collect(&mut s, a), None);
        }
        assert_eq!(collect(&mut s, a), Some(48));
        assert_eq!((s.bag.len(), s.karma), (MAX_BAG, 48));
    }

    #[test]
    fn junk_augments_are_cleaned_from_a_hero_file() {
        let mut s = hero();
        let ray = PowerId(10);
        let put = |power, slot, augment| SetAugment {
            power,
            slot,
            augment,
        };
        s.augments = vec![
            put(ray, 0, aug(Effect::Damage, 20)),
            // The same socket again, the same effect again, a socket too
            // many, an effect that does not fit, one far too strong, an
            // unknown effect, a power not owned.
            put(ray, 0, aug(Effect::Burn, 20)),
            put(ray, 1, aug(Effect::Damage, 20)),
            put(ray, 3, aug(Effect::Burn, 20)),
            put(ray, 1, aug(Effect::Heal, 20)),
            put(ray, 1, aug(Effect::Burn, 99)),
            put(
                ray,
                1,
                Augment {
                    effect: 200,
                    percent: 10,
                    grade: 0,
                },
            ),
            put(PowerId(12), 0, aug(Effect::Damage, 20)),
            put(ray, 2, aug(Effect::Hold, 20)),
        ];
        s.bag = vec![aug(Effect::Damage, 99); MAX_BAG + 5];
        s.bag[0] = aug(Effect::Damage, 20);
        sanitize(&mut s);
        assert_eq!(
            s.augments,
            vec![
                put(ray, 0, aug(Effect::Damage, 20)),
                put(ray, 2, aug(Effect::Hold, 20)),
            ]
        );
        assert_eq!(s.bag, vec![aug(Effect::Damage, 20)]);
    }

    #[test]
    fn drops_follow_rank_and_mostly_fit() {
        let s = hero();
        let fit = useful(&s);
        let mut counts = [0usize; 3];
        let mut grades = [[0usize; 3]; 3];
        let mut fitted = 0;
        let mut all = 0;
        let n = 20_000;
        for i in 0..n {
            for (rank, count) in counts.iter_mut().enumerate() {
                let mut dice = Dice::for_drop(i, rank as u32 + 7, 3);
                if let Some(a) = roll(&s, rank, 10, &mut dice) {
                    *count += 1;
                    grades[rank][a.grade as usize] += 1;
                    let e = Effect::of(a).expect("a known effect");
                    assert_eq!(a.percent, strength(Grade::of(a), 10));
                    fitted += fit.contains(&e) as usize;
                    all += 1;
                }
            }
        }
        for rank in 0..3 {
            let got = counts[rank] as f64 / n as f64;
            assert!((got - DROP_CHANCE[rank]).abs() < 0.01, "rank {rank}: {got}");
            for g in 0..3 {
                let got = grades[rank][g] as f64 / counts[rank].max(1) as f64;
                assert!(
                    (got - GRADE_CHANCE[rank][g]).abs() < 0.05,
                    "rank {rank} grade {g}: {got}"
                );
            }
        }
        // Seven in ten fit for sure; of the rest, some fit by chance.
        let share = fitted as f64 / all as f64;
        assert!((FITS_OWNED..0.95).contains(&share), "{share}");
        // A find is never common, and the same dice give the same find.
        for i in 0..200 {
            let a = find(&s, 12, &mut Dice::for_drop(i, 1, 2));
            assert!(a.grade >= 1);
            assert_eq!(a, find(&s, 12, &mut Dice::for_drop(i, 1, 2)));
        }
    }
}
