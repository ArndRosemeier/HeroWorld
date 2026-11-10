//! Factions: who likes whom. Every faction has a standing toward every
//! other one, from hostile to allied. A hero belongs to none: at the start
//! each faction regards a hero exactly as it regards townsfolk, so a new
//! hero is a nobody, and a gang treats them like any passer-by. Deeds move
//! that regard (kept on the hero's sheet as offsets): beating a gang's
//! people turns the gang against the hero and warms its rivals and victims.
//!
//! What a villain does follows from its faction's regard for the hero:
//!
//! - **Hostile:** attacks on sight.
//! - **Unfriendly:** on its own turf (a corner or roof in its quarter) it
//!   warns a hero off, and attacks one who comes too close or stays.
//!   Elsewhere it leaves the hero alone.
//! - **Neutral and better:** leaves the hero alone.
//!
//! Whatever the regard, a group fights back when a hero hits one of them,
//! and a crime's crooks turn on a hero who steps in (comes within the
//! crime's reach).
//!
//! Turf does not change hands in 1.0 (Arnd, 2026-11-08): borders are fixed.

use crate::combat::EnemyKind;
use hw_net::{Faction, FACTIONS};

/// Standing bands (regard from -100 to 100).
pub const HOSTILE: i16 = -60;
pub const UNFRIENDLY: i16 = -20;
pub const FRIENDLY: i16 = 20;
pub const ALLIED: i16 = 60;

/// How a faction stands toward someone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Attitude {
    Hostile,
    Unfriendly,
    Neutral,
    Friendly,
    Allied,
}

impl Attitude {
    pub fn of(regard: i16) -> Self {
        if regard <= HOSTILE {
            Attitude::Hostile
        } else if regard <= UNFRIENDLY {
            Attitude::Unfriendly
        } else if regard < FRIENDLY {
            Attitude::Neutral
        } else if regard < ALLIED {
            Attitude::Friendly
        } else {
            Attitude::Allied
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Attitude::Hostile => "Hostile",
            Attitude::Unfriendly => "Unfriendly",
            Attitude::Neutral => "Neutral",
            Attitude::Friendly => "Friendly",
            Attitude::Allied => "Allied",
        }
    }
}

/// How each faction (row) regards each other one (column), in the order of
/// [`Faction::ALL`]: Townsfolk, Patrol, Lab, Jackets, Syndicate, Moles, Sky
/// Pirates, Empire.
const TABLE: [[i16; FACTIONS]; FACTIONS] = [
    // Townsfolk fear the Lab and the Empire, mistrust the gangs and the
    // pirates, like the Patrol.
    [50, 40, -60, -30, -20, 0, -30, -80],
    // The Patrol keeps the peace against every gang and invader.
    [50, 80, -80, -50, -40, 0, -50, -90],
    // The Lab looks down on everyone; it hates the Patrol and the Syndicate
    // (rivals for the city's underworld), and the Empire (rivals for the
    // world).
    [-40, -70, 80, -30, -50, -40, -30, -60],
    // The Chrome Jackets: unfriendly to anyone outside the gang, at war
    // with the Syndicate.
    [-30, -60, -40, 80, -60, -10, -20, -60],
    // The Velvet Syndicate keeps the peace when it suits business; the
    // pirates are rival smugglers.
    [-10, -50, -50, -60, 80, 0, -40, -60],
    // Molehaven: wary of the surface, hates the Lab's diggers and the
    // Empire, which takes mole folk for its mines.
    [0, 10, -50, -10, -10, 80, 0, -70],
    // The sky pirates: rogues, not monsters. Unfriendly to landlubbers,
    // at odds with the law and the Syndicate; they trade with Molehaven.
    [-30, -60, -40, -10, -40, 10, 80, -50],
    // The Iron-Masked Empire: hostile to every power that is not of it,
    // and it despises townsfolk, but even it does not know a newcomer
    // yet (nobody attacks an unknown hero on sight): its legionaries
    // fight whoever comes near the people they round up.
    [-55, -90, -60, -60, -60, -70, -60, 90],
];

/// How `of` regards `toward`, before any deeds.
pub fn base(of: Faction, toward: Faction) -> i16 {
    TABLE[of.index()][toward.index()]
}

/// How `of` regards a hero whose deeds moved it by `regard` (the sheet's
/// offsets): as it regards townsfolk, plus the deeds.
pub fn toward_hero(of: Faction, regard: &[i8; FACTIONS]) -> i16 {
    (base(of, Faction::Townsfolk) + regard[of.index()] as i16).clamp(-100, 100)
}

/// The faction a villain belongs to.
pub fn of_enemy(kind: EnemyKind) -> Faction {
    match kind {
        EnemyKind::Brute
        | EnemyKind::Technician
        | EnemyKind::Sentry
        | EnemyKind::Alpha
        | EnemyKind::Colossus
        | EnemyKind::WeakPoint
        | EnemyKind::Doctor => Faction::Lab,
        EnemyKind::Greaser => Faction::Jackets,
        EnemyKind::Gunsel => Faction::Syndicate,
        EnemyKind::Delver => Faction::Moles,
        EnemyKind::Buccaneer | EnemyKind::Gunner | EnemyKind::Captain => Faction::SkyPirates,
        EnemyKind::Legionary | EnemyKind::Praetorian => Faction::Empire,
    }
}

/// Regard lost with a faction for each of its people a hero beats.
pub const BEATEN: i16 = 2;
/// Regard gained with each faction that dislikes the beaten one.
pub const RIVAL_GLAD: i16 = 1;

/// A hero beat one of `faction`'s people: that faction likes the hero less,
/// and every faction unfriendly to it (townsfolk and the Patrol included)
/// a little more.
pub fn beat(regard: &mut [i8; FACTIONS], faction: Faction) {
    for f in Faction::ALL {
        let d = if f == faction {
            -BEATEN
        } else if base(f, faction) <= UNFRIENDLY {
            RIVAL_GLAD
        } else {
            0
        };
        let r = &mut regard[f.index()];
        // Offsets keep the regard itself within -100..=100.
        let lo = -100 - base(f, Faction::Townsfolk);
        let hi = 100 - base(f, Faction::Townsfolk);
        *r = (*r as i16 + d).clamp(lo.max(-128), hi.min(127)) as i8;
    }
}

/// Regard gained with a faction whose frightened people a hero talked
/// down instead of fighting (mole folk surfacing).
pub const CALMED: i16 = 6;

/// A hero helped `faction`'s people (talked them down, sent them home
/// safe): the faction likes the hero better. Nobody else minds.
pub fn befriend(regard: &mut [i8; FACTIONS], faction: Faction, by: i16) {
    let f = faction.index();
    let lo = -100 - base(faction, Faction::Townsfolk);
    let hi = 100 - base(faction, Faction::Townsfolk);
    regard[f] = (regard[f] as i16 + by).clamp(lo.max(-128), hi.min(127)) as i8;
}

/// What a villain does about a hero it notices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reaction {
    Ignore,
    Warn,
    Attack,
}

/// A villain's state of mind toward one hero.
#[derive(Clone, Copy, Debug)]
pub struct Encounter {
    /// The villain's faction's attitude toward the hero.
    pub attitude: Attitude,
    /// How far the hero is (m).
    pub distance: f64,
    /// The villain stands on its own turf.
    pub turf: bool,
    /// A crime's reach: a hero this near has stepped in.
    pub crime: Option<f64>,
    /// How close a trespasser may come before the group attacks (m).
    pub trespass: f64,
    /// Seconds since the group warned this hero off, if it has.
    pub warned: Option<f64>,
}

/// Seconds a warned hero may stay before the group attacks.
pub const WARN_SECONDS: f64 = 6.0;
/// After this long a warning is forgotten (a hero who left and came back
/// is warned again).
pub const WARN_FORGET: f64 = 40.0;
/// A trespasser this close is attacked at once, warned or not (m).
pub const TRESPASS: f64 = 9.0;

pub fn react(e: Encounter) -> Reaction {
    if e.attitude == Attitude::Hostile {
        return Reaction::Attack;
    }
    if e.crime.is_some_and(|r| e.distance < r) {
        return Reaction::Attack;
    }
    if e.turf && e.attitude == Attitude::Unfriendly {
        if e.distance < e.trespass {
            return Reaction::Attack;
        }
        return match e.warned {
            Some(s) if s < WARN_FORGET => {
                if s >= WARN_SECONDS {
                    Reaction::Attack
                } else {
                    Reaction::Ignore
                }
            }
            _ => Reaction::Warn,
        };
    }
    Reaction::Ignore
}

/// A few lines a warning comes in, by faction.
pub fn warnings(f: Faction) -> &'static [&'static str] {
    match f {
        Faction::Lab => &[
            "Restricted area! Turn back!",
            "The Doctor's property. Leave. Now.",
            "Specimen spotted... no, just a nosy one. Scram!",
        ],
        Faction::Jackets => &[
            "Beat it, cape!",
            "Wrong street, pal.",
            "This is Jacket turf. Take a walk.",
        ],
        Faction::Syndicate => &[
            "Move along, friend.",
            "Nothing to see here.",
            "Private business. Keep walking.",
        ],
        Faction::Moles => &[
            "Back, surface-walker! Back!",
            "These tunnels are Molehaven's.",
            "No lamps past the gate, stranger.",
        ],
        Faction::SkyPirates => &[
            "Avast! This roof's spoken for!",
            "Shove off, landlubber!",
            "One more step and you walk the plank. Off the roof.",
        ],
        Faction::Empire => &[
            "Kneel before the Iron Throne!",
            "Resistance is noted. And punished.",
            "This city belongs to the Empire now.",
        ],
        _ => &["Go away!"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_empire_is_evil_and_the_others_are_not_all_bad() {
        let fresh = [0i8; FACTIONS];
        let att = |f| Attitude::of(toward_hero(f, &fresh));
        // Really evil, but nobody attacks an unknown hero on sight.
        assert_eq!(att(Faction::Empire), Attitude::Unfriendly);
        for f in Faction::ALL {
            assert!(
                base(Faction::Empire, Faction::Townsfolk) <= base(f, Faction::Townsfolk),
                "the Empire thinks least of townsfolk, not {f:?}"
            );
        }
        // Rogues: warn strangers off their turf, leave them alone elsewhere.
        assert_eq!(att(Faction::SkyPirates), Attitude::Unfriendly);
        // Molehaven minds its own business.
        assert_eq!(att(Faction::Moles), Attitude::Neutral);
        // Everyone hates the Empire, the Empire every power.
        for f in Faction::ALL {
            if f != Faction::Empire {
                assert!(base(f, Faction::Empire) <= UNFRIENDLY, "{f:?}");
            }
            if f != Faction::Empire && f != Faction::Townsfolk {
                assert!(base(Faction::Empire, f) <= HOSTILE, "{f:?}");
            }
        }
    }

    #[test]
    fn talking_mole_folk_down_wins_molehaven_over() {
        let mut regard = [0i8; FACTIONS];
        for _ in 0..4 {
            befriend(&mut regard, Faction::Moles, CALMED);
        }
        assert_eq!(
            Attitude::of(toward_hero(Faction::Moles, &regard)),
            Attitude::Friendly
        );
        for f in Faction::ALL {
            if f != Faction::Moles {
                assert_eq!(regard[f.index()], 0);
            }
        }
        for _ in 0..100 {
            befriend(&mut regard, Faction::Moles, CALMED);
        }
        assert_eq!(toward_hero(Faction::Moles, &regard), 100);
    }

    fn meet(attitude: Attitude, distance: f64) -> Encounter {
        Encounter {
            attitude,
            distance,
            turf: false,
            crime: None,
            trespass: TRESPASS,
            warned: None,
        }
    }

    #[test]
    fn a_new_hero_is_regarded_like_townsfolk() {
        let fresh = [0i8; FACTIONS];
        for f in Faction::ALL {
            assert_eq!(toward_hero(f, &fresh), base(f, Faction::Townsfolk));
        }
        // Nobody attacks a newcomer on sight.
        for f in Faction::ALL {
            assert_ne!(Attitude::of(toward_hero(f, &fresh)), Attitude::Hostile);
        }
    }

    #[test]
    fn gangs_ignore_a_passer_by_off_their_turf() {
        assert_eq!(
            react(meet(Attitude::Unfriendly, 5.0)),
            Reaction::Ignore,
            "an unfriendly gang off its turf leaves people alone"
        );
        assert_eq!(react(meet(Attitude::Hostile, 15.0)), Reaction::Attack);
    }

    #[test]
    fn turf_is_warned_then_defended() {
        let on_turf = |distance, warned| Encounter {
            turf: true,
            warned,
            ..meet(Attitude::Unfriendly, distance)
        };
        assert_eq!(react(on_turf(15.0, None)), Reaction::Warn);
        assert_eq!(react(on_turf(15.0, Some(2.0))), Reaction::Ignore);
        assert_eq!(react(on_turf(15.0, Some(WARN_SECONDS))), Reaction::Attack);
        assert_eq!(react(on_turf(5.0, None)), Reaction::Attack);
        assert_eq!(
            react(on_turf(15.0, Some(WARN_FORGET + 1.0))),
            Reaction::Warn
        );
        // A neutral faction lets people pass even on its turf.
        let neutral = Encounter {
            attitude: Attitude::Neutral,
            ..on_turf(5.0, None)
        };
        assert_eq!(react(neutral), Reaction::Ignore);
    }

    #[test]
    fn crooks_turn_on_a_hero_who_steps_in() {
        let crime = |distance| Encounter {
            crime: Some(8.0),
            ..meet(Attitude::Neutral, distance)
        };
        assert_eq!(react(crime(12.0)), Reaction::Ignore);
        assert_eq!(react(crime(6.0)), Reaction::Attack);
    }

    #[test]
    fn beating_a_gang_turns_it_hostile_and_pleases_its_rivals() {
        let mut regard = [0i8; FACTIONS];
        let mut beaten = 0;
        while Attitude::of(toward_hero(Faction::Jackets, &regard)) != Attitude::Hostile {
            beat(&mut regard, Faction::Jackets);
            beaten += 1;
            assert!(beaten < 100);
        }
        // About three groups of them.
        assert!((10..=20).contains(&beaten), "{beaten}");
        assert!(
            toward_hero(Faction::Syndicate, &regard) > base(Faction::Syndicate, Faction::Townsfolk)
        );
        assert!(toward_hero(Faction::Townsfolk, &regard) > 50);
        // Molehaven only mildly minds the Jackets: no change.
        assert_eq!(regard[Faction::Moles.index()], 0);
        assert_eq!(regard[Faction::SkyPirates.index()], 0);
        // Regard stays in range however many are beaten.
        for _ in 0..500 {
            beat(&mut regard, Faction::Jackets);
        }
        for f in Faction::ALL {
            assert!((-100..=100).contains(&toward_hero(f, &regard)));
        }
    }
}
