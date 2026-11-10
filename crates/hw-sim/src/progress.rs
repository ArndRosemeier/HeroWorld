//! Karma, levels and powers: the rules of a hero's progress (milestone 7).
//!
//! Karma is the one currency. All karma ever earned (`lifetime`) sets the
//! level; karma not spent yet buys powers. A power's price depends on its
//! tier, on how many powers the hero has already and on the hero's leaning
//! toward the power's affinity; giving a power up returns 90% of what was
//! paid. There are no power ranks: what a power does follows from the
//! hero's level (see [`crate::combat::level_scale`]).
//!
//! Pacing (design doc): one level costs about one tier-1 power, level 2
//! comes after about ten minutes of play, level 5 (flight) after about an
//! hour, level 20 after about ten. A minion is worth six or seven karma, and
//! a hero beats about ten a minute, walking between street corners
//! (`tests/powers.rs` plays it).

use hw_net::{OwnedPower, Power, Sheet};

/// The highest level.
pub const MAX_LEVEL: u8 = 20;
/// The level each power tier opens at (tier 1 at level 1, ...).
pub const TIER_LEVELS: [u8; 7] = [1, 3, 5, 8, 12, 16, 20];
/// Karma from level 1 to 2; each later level costs a quarter of that more.
pub const FIRST_LEVEL_KARMA: f64 = 960.0;
/// Past level 5 each level costs this much more again per level above 5:
/// a hero with many powers beats villains much faster, and level 20 should
/// still take about ten hours.
pub const LATE_LEVEL_STEP: f64 = 0.04;
/// A tier-1 power with no others owned costs about one level's karma.
pub const TIER1_PRICE: f64 = FIRST_LEVEL_KARMA;
/// Each tier's price in tier-1 prices: half again per tier up to tier 3,
/// then a smaller step, so the top tiers stay within reach of the levels
/// that open them (a level costs about five times as much at level 19 as
/// at level 1, while every owned power adds its markup).
pub const TIER_PRICES: [f64; 7] = [1.0, 1.5, 2.25, 2.75, 3.25, 3.75, 4.25];
/// Each power already owned makes the next a quarter dearer.
pub const OWNED_MARKUP: f64 = 0.25;
/// A new hero starts with enough karma for one tier-1 power.
pub const START_KARMA: u32 = TIER1_PRICE as u32;
/// What comes back when a power is given up.
pub const REFUND: f64 = 0.9;
/// The largest discount a strong affinity gives.
pub const MAX_DISCOUNT: f64 = 0.30;
/// Affinity points that count as "no leaning yet": the discount is
/// `MAX_DISCOUNT * points / (all points + this)`, so it grows with deeds.
pub const AFFINITY_DAMPING: f64 = 1200.0;
/// The push an origin gives its affinity.
pub const ORIGIN_PUSH: u32 = 600;

/// Where a hero's powers lean, grown by deeds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Affinity {
    /// Rescues, revives, shielding friends.
    Guardian,
    /// Fighting crime and villains.
    Justice,
    /// Ray guns, force fields, gadgets.
    Science,
    /// Sigils, spirits, the Aether itself.
    Mystic,
    /// Muscle.
    Might,
}

impl Affinity {
    pub const ALL: [Affinity; 5] = [
        Affinity::Guardian,
        Affinity::Justice,
        Affinity::Science,
        Affinity::Mystic,
        Affinity::Might,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Affinity::Guardian => "Guardian",
            Affinity::Justice => "Justice",
            Affinity::Science => "Science",
            Affinity::Mystic => "Mystic",
            Affinity::Might => "Might",
        }
    }
}

/// How a hero got their powers: picked once, at creation. It gives its
/// affinity the first push.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Origin {
    pub name: &'static str,
    pub blurb: &'static str,
    pub affinity: Affinity,
}

pub const ORIGINS: [Origin; 5] = [
    Origin {
        name: "Lab Accident",
        blurb: "A beaker of raw Aether went off in your face. Science!",
        affinity: Affinity::Science,
    },
    Origin {
        name: "Ancient Relic",
        blurb: "You found an amulet older than the moon. It found you back.",
        affinity: Affinity::Mystic,
    },
    Origin {
        name: "Born Mighty",
        blurb: "You lifted the family car at six. Mostly by accident.",
        affinity: Affinity::Might,
    },
    Origin {
        name: "Sworn Guardian",
        blurb: "You promised someone you would keep this city safe.",
        affinity: Affinity::Guardian,
    },
    Origin {
        name: "Masked Avenger",
        blurb: "The mad scientist's gang crossed the wrong family.",
        affinity: Affinity::Justice,
    },
];

/// One way a power looks and feels: the mechanic stays, the theme picks
/// the look, the name and the affinity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub name: &'static str,
    pub blurb: &'static str,
    /// None for movement powers (no discount, no leaning).
    pub affinity: Option<Affinity>,
}

/// The two themes of a power.
pub fn themes(p: Power) -> [Theme; 2] {
    use Affinity::*;
    let t = |name, blurb, affinity| Theme {
        name,
        blurb,
        affinity,
    };
    match p {
        Power::Strike => [
            t("Strike", "A good honest punch.", Some(Might)),
            t("Strike", "A good honest punch.", Some(Might)),
        ],
        Power::Bolt => [
            t(
                "Atomic Ray",
                "A crackling blue beam from your palm.",
                Some(Science),
            ),
            t(
                "Arcane Bolt",
                "A violet sigil that bursts on impact.",
                Some(Mystic),
            ),
        ],
        Power::SuperJump => [
            t(
                "Spring Heels",
                "Legs like coiled steel: clear a house.",
                None,
            ),
            t("Rocket Boots", "Little fins, big flames.", None),
        ],
        Power::Hold => [
            t(
                "Gravity Cuffs",
                "Rings of force pin a villain in place.",
                Some(Science),
            ),
            t(
                "Binding Sigils",
                "Glowing runes wrap them like rope.",
                Some(Mystic),
            ),
        ],
        Power::Shield => [
            t(
                "Force Field",
                "A humming bubble for you and friends nearby.",
                Some(Science),
            ),
            t(
                "Guardian Aegis",
                "A golden ward over everyone at your side.",
                Some(Guardian),
            ),
        ],
        Power::Lift => [
            t(
                "Mighty Heave",
                "Pick it up. Throw it at a villain.",
                Some(Might),
            ),
            t(
                "Telekinesis",
                "Float it up with your mind, then let fly.",
                Some(Mystic),
            ),
        ],
        Power::Flight => [
            t("Jet Pack", "Strapped-on chrome and a roar.", None),
            t("Soaring Cape", "Up, up and away.", None),
        ],
        Power::Shockwave => [
            t(
                "Thunderclap",
                "Clap your hands; everyone near falls over.",
                Some(Might),
            ),
            t(
                "Sonic Boom",
                "A ring of pressure bowls a crowd over.",
                Some(Science),
            ),
        ],
        Power::SuperSpeed => [
            t(
                "Lightning Legs",
                "A blur, a crackle, and you're three blocks on.",
                None,
            ),
            t(
                "Rocket Skates",
                "Chrome skates with tail fins. Mind the corners.",
                None,
            ),
        ],
        Power::Mend => [
            t(
                "Vital Ray",
                "A warm green beam knits wounds while you watch.",
                Some(Science),
            ),
            t(
                "Healing Light",
                "A golden glow over you and everyone at your side.",
                Some(Guardian),
            ),
        ],
        Power::Beam => [
            t(
                "Disintegrator",
                "A ruler-straight ray that goes through the whole gang.",
                Some(Science),
            ),
            t(
                "Spirit Lance",
                "A spear of pale fire that pierces every foe in line.",
                Some(Mystic),
            ),
        ],
        Power::Vitality => [
            t(
                "Aether Vitality",
                "The Aether mends you as fast as they can hurt you.",
                Some(Mystic),
            ),
            t(
                "Iron Constitution",
                "Shrug it off. Walk it off. Keep punching.",
                Some(Might),
            ),
        ],
        Power::Meteor => [
            t(
                "Orbital Ray",
                "Your satellite answers: a white-hot column from the sky.",
                Some(Science),
            ),
            t(
                "Starfall",
                "Pull a falling star down on their heads.",
                Some(Mystic),
            ),
        ],
        Power::Invulnerable => [
            t(
                "Steel Hide",
                "Bullets bounce. Fists bruise themselves.",
                Some(Might),
            ),
            t(
                "Unbreakable Will",
                "Justice does not fall down.",
                Some(Justice),
            ),
        ],
        Power::Nova => [
            t(
                "Atomic Nova",
                "Become a small sun for a moment. Everyone near regrets it.",
                Some(Science),
            ),
            t(
                "Earthshaker",
                "Hit the ground so hard the street jumps.",
                Some(Might),
            ),
        ],
        Power::Rally => [
            t(
                "Rallying Cry",
                "Friends near you fight harder and tire slower.",
                Some(Justice),
            ),
            t(
                "Heroic Aura",
                "A glow of pure courage round you and your team.",
                Some(Guardian),
            ),
        ],
        Power::Freeze => [
            t(
                "Cryo Ray",
                "A frosty blast from a chrome nozzle. Hoodlums on the rocks.",
                Some(Science),
            ),
            t(
                "Frost Sigil",
                "A rune of winter: breath freezes, then everything else.",
                Some(Mystic),
            ),
        ],
        Power::Teleport => [
            t(
                "Matter Transmitter",
                "A hum, a sparkle, and you're over there.",
                Some(Science),
            ),
            t(
                "Aether Step",
                "Step through a fold in the Aether.",
                Some(Mystic),
            ),
        ],
        Power::Bubble => [
            t(
                "Force Bubble",
                "A floating sphere of force carries them out of harm's way.",
                Some(Science),
            ),
            t(
                "Guardian Orb",
                "A golden orb lifts the helpless to safety.",
                Some(Guardian),
            ),
        ],
        Power::Sidekick => [
            t(
                "Tin Pal",
                "A pint-sized chrome robot with a big heart and bigger fists.",
                Some(Science),
            ),
            t(
                "Brass Squire",
                "A loyal little brass knight-robot at your side.",
                Some(Guardian),
            ),
        ],
        Power::Hypno => [
            t(
                "Hypno Spiral",
                "Look into the spiral. You are now on our side.",
                Some(Science),
            ),
            t(
                "Charm",
                "A whisper from the Aether turns a foe into a friend.",
                Some(Mystic),
            ),
        ],
    }
}

/// A power's tier (1 = the first ones).
pub fn tier(p: Power) -> u8 {
    match p {
        Power::Strike | Power::Bolt | Power::SuperJump => 1,
        Power::Hold | Power::Shield | Power::Lift | Power::Freeze => 2,
        Power::Flight | Power::Shockwave | Power::Teleport | Power::Bubble => 3,
        Power::SuperSpeed | Power::Mend | Power::Sidekick => 4,
        Power::Beam | Power::Vitality | Power::Hypno => 5,
        Power::Meteor | Power::Invulnerable => 6,
        Power::Nova | Power::Rally => 7,
    }
}

/// A movement power: it changes how the hero moves, and has no key.
pub fn is_movement(p: Power) -> bool {
    matches!(p, Power::SuperJump | Power::Flight | Power::SuperSpeed)
}

/// Always on once owned: movement powers and the passive ones. Everything
/// else is used with its key.
pub fn is_passive(p: Power) -> bool {
    is_movement(p) || matches!(p, Power::Vitality | Power::Invulnerable | Power::Rally)
}

/// The level a tier opens at.
pub fn tier_level(tier: u8) -> u8 {
    TIER_LEVELS[(tier.clamp(1, 7) - 1) as usize]
}

/// Karma it takes to go from `level` to the next.
pub fn karma_to_next(level: u8) -> u32 {
    let l = level.clamp(1, MAX_LEVEL) as f64;
    let late = 1.0 + LATE_LEVEL_STEP * (l - 5.0).max(0.0);
    (FIRST_LEVEL_KARMA * (1.0 + 0.25 * (l - 1.0)) * late).round() as u32
}

/// Lifetime karma at which a hero reaches `level`.
pub fn karma_for_level(level: u8) -> u32 {
    (1..level.clamp(1, MAX_LEVEL)).map(karma_to_next).sum()
}

/// The level lifetime karma gives.
pub fn level_of(lifetime: u32) -> u8 {
    let mut level = 1;
    while level < MAX_LEVEL && lifetime >= karma_for_level(level + 1) {
        level += 1;
    }
    level
}

/// The share off a power's price from leaning toward `a` (0 to 0.3).
pub fn discount(sheet: &Sheet, a: Affinity) -> f64 {
    let all: f64 = sheet.affinity.iter().map(|&x| x as f64).sum();
    let mine = sheet.affinity[a.index()] as f64;
    MAX_DISCOUNT * mine / (all + AFFINITY_DAMPING)
}

/// What `power` in `theme` costs this hero now.
pub fn price(sheet: &Sheet, power: Power, theme: u8) -> u32 {
    let base = TIER1_PRICE * TIER_PRICES[(tier(power).clamp(1, 7) - 1) as usize];
    let owned = sheet.powers.len() as f64;
    let off = themes(power)[(theme & 1) as usize]
        .affinity
        .map_or(0.0, |a| discount(sheet, a));
    (base * (1.0 + OWNED_MARKUP * owned) * (1.0 - off)).round() as u32
}

/// Why a power cannot be bought.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuyError {
    AlreadyOwned,
    /// Its tier opens at this level.
    TooLow(u8),
    /// It costs this much.
    TooDear(u32),
    /// Strike is everyone's.
    NotForSale,
}

impl std::fmt::Display for BuyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuyError::AlreadyOwned => write!(f, "you have it already"),
            BuyError::TooLow(l) => write!(f, "opens at level {l}"),
            BuyError::TooDear(k) => write!(f, "costs {k} karma"),
            BuyError::NotForSale => write!(f, "everyone has it"),
        }
    }
}

/// Can this hero buy `power` now? Returns the price.
pub fn can_buy(sheet: &Sheet, power: Power, theme: u8) -> Result<u32, BuyError> {
    if power == Power::Strike {
        return Err(BuyError::NotForSale);
    }
    if sheet.has(power) {
        return Err(BuyError::AlreadyOwned);
    }
    let opens = tier_level(tier(power));
    if level_of(sheet.lifetime) < opens {
        return Err(BuyError::TooLow(opens));
    }
    let cost = price(sheet, power, theme);
    if sheet.karma < cost {
        return Err(BuyError::TooDear(cost));
    }
    Ok(cost)
}

/// Buys `power` in `theme`; returns what it cost.
pub fn buy(sheet: &mut Sheet, power: Power, theme: u8) -> Result<u32, BuyError> {
    let cost = can_buy(sheet, power, theme)?;
    sheet.karma -= cost;
    sheet.powers.push(OwnedPower {
        power,
        theme: theme & 1,
        paid: cost,
    });
    Ok(cost)
}

/// Gives `power` up for 90% of what was paid; returns the refund.
pub fn give_up(sheet: &mut Sheet, power: Power) -> Option<u32> {
    let i = sheet.powers.iter().position(|p| p.power == power)?;
    let p = sheet.powers.remove(i);
    let back = (p.paid as f64 * REFUND).floor() as u32;
    sheet.karma += back;
    Some(back)
}

/// A new hero of `origin` (an index into [`ORIGINS`]): no powers yet but
/// Strike, karma for one tier-1 power, and the origin's push.
pub fn new_hero(origin: u8) -> Sheet {
    let origin = origin.min(ORIGINS.len() as u8 - 1);
    let mut s = Sheet {
        karma: START_KARMA,
        origin,
        ..Default::default()
    };
    s.affinity[ORIGINS[origin as usize].affinity.index()] = ORIGIN_PUSH;
    s
}

/// The hero a player plays before their hero file arrives (and in tests):
/// Lab Accident, with Energy Bolt bought from the starting karma.
pub fn starter() -> Sheet {
    let mut s = new_hero(0);
    let _ = buy(&mut s, Power::Bolt, 0);
    s
}

/// Earns `karma`; half of it leans toward `deed`'s affinity. Returns the
/// new level if it went up.
pub fn earn(sheet: &mut Sheet, karma: u32, deed: Option<Affinity>) -> Option<u8> {
    let before = level_of(sheet.lifetime);
    sheet.lifetime = sheet.lifetime.saturating_add(karma);
    sheet.karma = sheet.karma.saturating_add(karma);
    if let Some(a) = deed {
        sheet.affinity[a.index()] = sheet.affinity[a.index()].saturating_add(karma.div_ceil(2));
    }
    let after = level_of(sheet.lifetime);
    (after > before).then_some(after)
}

/// Makes a sheet from a player's hero file sane: known powers once each,
/// Strike not listed, a known origin.
pub fn sanitize(mut s: Sheet) -> Sheet {
    let mut seen = Vec::new();
    s.powers.retain(|p| {
        let keep = p.power != Power::Strike && !seen.contains(&p.power);
        seen.push(p.power);
        keep
    });
    for p in s.powers.iter_mut() {
        p.theme &= 1;
    }
    s.origin = s.origin.min(ORIGINS.len() as u8 - 1);
    s.powers.truncate(hw_net::protocol::MAX_POWERS);
    s.story = s.story.min(crate::story::CHAPTERS.len() as u8);
    s.briefed &= (s.story as usize) < crate::story::CHAPTERS.len();
    s
}

/// Karma for defeating a villain of `level` (base `karma` for an even
/// match) at the hero's `level`: more for tougher ones, little for grey.
pub fn defeat_karma(base: f32, villain: u8, hero: u8) -> u32 {
    let diff = villain as f32 - hero as f32;
    let k = (1.0 + 0.25 * diff).clamp(0.2, 2.0);
    (base * k).round().max(1.0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn levels_follow_lifetime_karma() {
        assert_eq!(level_of(0), 1);
        assert_eq!(karma_to_next(1), 960);
        assert_eq!(karma_to_next(2), 1200);
        assert_eq!(karma_for_level(2), 960);
        assert_eq!(karma_for_level(3), 2160);
        assert_eq!(level_of(959), 1);
        assert_eq!(level_of(960), 2);
        assert_eq!(level_of(2159), 2);
        assert_eq!(level_of(2160), 3);
        assert_eq!(level_of(u32::MAX), MAX_LEVEL);
        // Levels 1 to 5 keep their pacing (an hour to flight); later ones
        // grow steeper. `tests/high_tiers.rs` plays the ten hours to 20.
        assert_eq!(karma_to_next(5), 1920);
        assert_eq!(karma_to_next(6), 2246);
        assert!((1..MAX_LEVEL).all(|l| karma_to_next(l + 1) > karma_to_next(l)));
        let total = karma_for_level(MAX_LEVEL);
        assert!((75_000..80_000).contains(&total), "{total}");
    }

    #[test]
    #[ignore = "on demand (balance): scripts/check.sh --on-demand"]
    fn one_level_buys_about_one_first_tier_power() {
        let s = new_hero(0);
        assert_eq!(price(&s, Power::SuperJump, 0), karma_to_next(1));
        assert!(can_buy(&s, Power::SuperJump, 0).is_ok());
        // A tier-2 power is half again, tier 3 more.
        assert_eq!(price(&s, Power::Hold, 1) as f64, TIER1_PRICE * 1.5);
        // A Lab Accident hero leans to Science: its themes are cheaper.
        assert!(price(&s, Power::Hold, 0) < price(&s, Power::Hold, 1));
        assert!(price(&s, Power::Flight, 0) > price(&s, Power::Lift, 0));
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn prices_grow_with_powers_owned_and_fall_with_affinity() {
        let mut s = new_hero(2); // Born Mighty
        s.karma = 10_000;
        s.lifetime = karma_for_level(5);
        let before = price(&s, Power::SuperJump, 0);
        buy(&mut s, Power::SuperJump, 0).unwrap();
        let after = price(&s, Power::Flight, 0);
        assert!(after as f64 > before as f64 * 1.5 * 1.5 * 1.2);
        // Might leaning makes the Might theme cheaper than the Science one.
        let might = price(&s, Power::Shockwave, 0);
        let science = price(&s, Power::Shockwave, 1);
        assert!(might < science, "{might} vs {science}");
        // Movement powers get no discount.
        assert_eq!(price(&s, Power::Flight, 0), price(&s, Power::Flight, 1));
        // The discount never passes 30%.
        s.affinity = [0, 0, 0, 0, 1_000_000];
        let full = price(&s, Power::Shockwave, 0) as f64;
        let none = price(&s, Power::Shockwave, 1) as f64;
        assert!(full >= none * 0.69 && full < none * 0.72, "{full} {none}");
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn tiers_open_by_level_and_karma_must_suffice() {
        let mut s = new_hero(0);
        assert_eq!(can_buy(&s, Power::Hold, 0), Err(BuyError::TooLow(3)));
        assert_eq!(can_buy(&s, Power::Flight, 0), Err(BuyError::TooLow(5)));
        assert_eq!(can_buy(&s, Power::Strike, 0), Err(BuyError::NotForSale));
        s.lifetime = karma_for_level(3);
        s.karma = 10;
        assert!(matches!(
            can_buy(&s, Power::Hold, 0),
            Err(BuyError::TooDear(_))
        ));
        s.karma = 5000;
        buy(&mut s, Power::Hold, 1).unwrap();
        assert_eq!(can_buy(&s, Power::Hold, 0), Err(BuyError::AlreadyOwned));
        assert_eq!(s.theme(Power::Hold), 1);
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn giving_a_power_up_returns_ninety_percent() {
        let mut s = new_hero(0);
        let cost = buy(&mut s, Power::Bolt, 0).unwrap();
        assert_eq!(s.karma, START_KARMA - cost);
        let back = give_up(&mut s, Power::Bolt).unwrap();
        assert_eq!(back, (cost as f64 * 0.9) as u32);
        assert!(!s.has(Power::Bolt));
        assert_eq!(give_up(&mut s, Power::Bolt), None);
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn earning_karma_levels_up_and_leans() {
        let mut s = new_hero(4);
        assert_eq!(earn(&mut s, 400, Some(Affinity::Justice)), None);
        assert_eq!(earn(&mut s, 560, Some(Affinity::Might)), Some(2));
        assert_eq!(s.lifetime, 960);
        assert_eq!(s.affinity[Affinity::Justice.index()], ORIGIN_PUSH + 200);
        assert_eq!(s.affinity[Affinity::Might.index()], 280);
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn tougher_villains_pay_more() {
        assert_eq!(defeat_karma(10.0, 3, 3), 10);
        assert!(defeat_karma(10.0, 4, 3) > 10);
        assert!(defeat_karma(10.0, 1, 5) < 5);
        assert!(defeat_karma(10.0, 1, 20) >= 1);
    }

    #[test]
    fn junk_hero_files_are_cleaned() {
        let s = sanitize(Sheet {
            origin: 99,
            powers: vec![
                OwnedPower {
                    power: Power::Bolt,
                    theme: 7,
                    paid: 1,
                },
                OwnedPower {
                    power: Power::Bolt,
                    theme: 0,
                    paid: 1,
                },
                OwnedPower {
                    power: Power::Strike,
                    theme: 0,
                    paid: 1,
                },
            ],
            ..Default::default()
        });
        assert_eq!(s.origin, 4);
        assert_eq!(s.powers.len(), 1);
        assert_eq!(s.powers[0].theme, 1);
    }
}
