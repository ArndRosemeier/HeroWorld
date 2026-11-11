//! Karma, levels and powers: the rules of a hero's progress.
//!
//! Karma is the one currency. All karma ever earned (`lifetime`) sets the
//! level; karma not spent yet buys powers. Powers sit in trees (see
//! [`crate::powers`]): a hero buys from their origin's tree and the travel
//! tree, a power once what it needs is owned. Its price goes by its row and
//! by how many powers the hero has already; giving a power up returns 90%
//! of what was paid, once nothing still owned needs it. What a power does
//! follows from the hero's level (see [`crate::combat::level_scale`]).
//!
//! Pacing: one level costs about one first-row power, level 2 comes after
//! about ten minutes of play, level 20 after about ten hours, with about
//! thirteen powers owned by then (half a tree and a few travel powers).
//! A minion is worth six or seven karma, and a hero beats about ten a
//! minute, walking between street corners (`tests/powers.rs` plays it).

use crate::powers::{self, Element, Tree};
use hw_net::{OwnedPower, PowerId, Sheet};

/// The highest level.
pub const MAX_LEVEL: u8 = 20;
/// Karma from level 1 to 2; each later level costs a quarter of that more.
pub const FIRST_LEVEL_KARMA: f64 = 960.0;
/// Past level 5 each level costs this much more again per level above 5:
/// a hero with many powers beats villains much faster, and level 20 should
/// still take about ten hours.
pub const LATE_LEVEL_STEP: f64 = 0.04;
/// A first-row power with no others owned costs about one level's karma.
pub const ROW1_PRICE: f64 = FIRST_LEVEL_KARMA;
/// Each row's price in first-row prices.
pub const ROW_PRICES: [f64; 6] = [1.0, 1.5, 2.25, 2.75, 3.25, 4.25];
/// Each power already owned makes the next a quarter dearer.
pub const OWNED_MARKUP: f64 = 0.25;
/// A new hero starts with enough karma for one first-row power.
pub const START_KARMA: u32 = ROW1_PRICE as u32;
/// What comes back when a power is given up.
pub const REFUND: f64 = 0.9;

/// How a hero got their powers: picked once, at creation. It decides the
/// tree they buy from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Origin {
    pub name: &'static str,
    pub blurb: &'static str,
    pub tree: Tree,
}

pub const ORIGINS: [Origin; 5] = [
    Origin {
        name: "Lab Accident",
        blurb: "A beaker of raw Aether went off in your face. Science!",
        tree: Tree::Science,
    },
    Origin {
        name: "Ancient Relic",
        blurb: "You found an amulet older than the moon. It found you back.",
        tree: Tree::Mystic,
    },
    Origin {
        name: "Born Mighty",
        blurb: "You lifted the family car at six. Mostly by accident.",
        tree: Tree::Might,
    },
    Origin {
        name: "Sworn Guardian",
        blurb: "You promised someone you would keep this city safe.",
        tree: Tree::Guardian,
    },
    Origin {
        name: "Masked Avenger",
        blurb: "The mad scientist's gang crossed the wrong family.",
        tree: Tree::Justice,
    },
];

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

/// What `power` costs this hero now.
pub fn price(sheet: &Sheet, power: PowerId) -> u32 {
    let row = powers::def_of(power).row.min(5) as usize;
    let owned = sheet.powers.len() as f64;
    (ROW1_PRICE * ROW_PRICES[row] * (1.0 + OWNED_MARKUP * owned)).round() as u32
}

/// Why a power cannot be bought.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuyError {
    AlreadyOwned,
    /// Strike is everyone's; an unknown number is nobody's.
    NotForSale,
    /// Another origin's tree.
    OtherTree,
    /// It needs this power first (one of them, if several would do).
    Needs(PowerId),
    /// It needs this many more powers of its tree first.
    NeedsMore(u8),
    /// The other side of its fork was picked, for good.
    ForkTaken(PowerId),
    /// It costs this much.
    TooDear(u32),
}

impl std::fmt::Display for BuyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuyError::AlreadyOwned => write!(f, "you have it already"),
            BuyError::NotForSale => write!(f, "everyone has it"),
            BuyError::OtherTree => write!(f, "another origin's power"),
            BuyError::Needs(p) => write!(f, "needs {} first", powers::def_of(*p).name),
            BuyError::NeedsMore(n) => write!(f, "needs {n} more travel powers first"),
            BuyError::ForkTaken(p) => write!(f, "you took {} instead", powers::def_of(*p).name),
            BuyError::TooDear(k) => write!(f, "costs {k} karma"),
        }
    }
}

/// Is `power` open to this hero (their trees, what it needs met, its fork
/// free), whatever it costs?
pub fn open(sheet: &Sheet, power: PowerId) -> Result<(), BuyError> {
    let Some(d) = powers::get(power).filter(|d| !powers::is_free(d.id)) else {
        return Err(BuyError::NotForSale);
    };
    if sheet.has(power) {
        return Err(BuyError::AlreadyOwned);
    }
    if d.tree != Tree::Common && d.tree != Tree::of_origin(sheet.origin) {
        return Err(BuyError::OtherTree);
    }
    if let Some(first) = unmet(sheet, d) {
        return Err(first);
    }
    if d.fork != 0 {
        if let Some(o) = sheet.powers.iter().find(|o| {
            let od = powers::def_of(o.power);
            od.tree == d.tree && od.fork == d.fork
        }) {
            return Err(BuyError::ForkTaken(o.power));
        }
    }
    Ok(())
}

/// The first of `d`'s needs this hero has not met, if any.
fn unmet(sheet: &Sheet, d: &powers::PowerDef) -> Option<BuyError> {
    for any in d.needs {
        if !any.iter().any(|&n| sheet.has(PowerId(n))) {
            return Some(BuyError::Needs(PowerId(any[0])));
        }
    }
    let mine = sheet
        .powers
        .iter()
        .filter(|o| o.power != d.id && powers::def_of(o.power).tree == d.tree)
        .count() as u8;
    (mine < d.needs_any).then(|| BuyError::NeedsMore(d.needs_any - mine))
}

/// Can this hero buy `power` now? Returns the price.
pub fn can_buy(sheet: &Sheet, power: PowerId) -> Result<u32, BuyError> {
    open(sheet, power)?;
    let cost = price(sheet, power);
    if sheet.karma < cost {
        return Err(BuyError::TooDear(cost));
    }
    Ok(cost)
}

/// Buys `power`; returns what it cost.
pub fn buy(sheet: &mut Sheet, power: PowerId) -> Result<u32, BuyError> {
    let cost = can_buy(sheet, power)?;
    sheet.karma -= cost;
    sheet.powers.push(OwnedPower { power, paid: cost });
    Ok(cost)
}

/// Why a power cannot be given up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GiveUpError {
    NotOwned,
    /// This owned power needs it: give that up first.
    Needed(PowerId),
}

impl std::fmt::Display for GiveUpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GiveUpError::NotOwned => write!(f, "you do not have it"),
            GiveUpError::Needed(p) => {
                write!(
                    f,
                    "{} needs it: give that up first",
                    powers::def_of(*p).name
                )
            }
        }
    }
}

/// Can `power` be given up (nothing still owned needs it)?
pub fn can_give_up(sheet: &Sheet, power: PowerId) -> Result<(), GiveUpError> {
    if !sheet.powers.iter().any(|p| p.power == power) {
        return Err(GiveUpError::NotOwned);
    }
    let mut without = sheet.clone();
    without.powers.retain(|p| p.power != power);
    for o in &without.powers {
        if unmet(&without, powers::def_of(o.power)).is_some() {
            return Err(GiveUpError::Needed(o.power));
        }
    }
    Ok(())
}

/// Gives `power` up for 90% of what was paid; returns the refund.
pub fn give_up(sheet: &mut Sheet, power: PowerId) -> Result<u32, GiveUpError> {
    can_give_up(sheet, power)?;
    let i = sheet
        .powers
        .iter()
        .position(|p| p.power == power)
        .ok_or(GiveUpError::NotOwned)?;
    let p = sheet.powers.remove(i);
    let back = (p.paid as f64 * REFUND).floor() as u32;
    sheet.karma += back;
    Ok(back)
}

/// Gives every power back for all that was paid (once, when the power
/// trees came: old heroes pick again).
pub fn refund_all(sheet: &mut Sheet) {
    let paid: u32 = sheet.powers.iter().map(|p| p.paid).sum();
    sheet.karma = sheet.karma.saturating_add(paid);
    sheet.powers.clear();
}

/// A new hero of `origin` (an index into [`ORIGINS`]) and `element`: no
/// powers yet but Strike, and karma for one first-row power.
pub fn new_hero(origin: u8, element: Element) -> Sheet {
    Sheet {
        karma: START_KARMA,
        origin: origin.min(ORIGINS.len() as u8 - 1),
        element: element.code(),
        ..Default::default()
    }
}

/// The first power of `origin`'s tree (its top-left attack).
pub fn first_power(origin: u8) -> PowerId {
    powers::tree(Tree::of_origin(origin))
        .filter(|d| d.row == 0 && !d.helper)
        .map(|d| d.id)
        .next()
        .unwrap_or(powers::STRIKE)
}

/// The hero a player plays before their hero file arrives (and in tests):
/// a Lab Accident of fire, with Atomic Ray bought from the starting karma.
pub fn starter() -> Sheet {
    let mut s = new_hero(0, Element::Fire);
    let _ = buy(&mut s, first_power(0));
    s
}

/// Earns `karma`. Returns the new level if it went up.
pub fn earn(sheet: &mut Sheet, karma: u32) -> Option<u8> {
    let before = level_of(sheet.lifetime);
    sheet.lifetime = sheet.lifetime.saturating_add(karma);
    sheet.karma = sheet.karma.saturating_add(karma);
    let after = level_of(sheet.lifetime);
    (after > before).then_some(after)
}

/// Makes a sheet from a player's hero file sane: known powers of the
/// hero's trees once each, one side of each fork, Strike not listed, a
/// known origin and element.
pub fn sanitize(mut s: Sheet) -> Sheet {
    s.origin = s.origin.min(ORIGINS.len() as u8 - 1);
    s.element = Element::from_code(s.element).code();
    let mine = Tree::of_origin(s.origin);
    let mut kept: Vec<OwnedPower> = Vec::new();
    for p in &s.powers {
        let Some(d) = powers::get(p.power).filter(|d| !powers::is_free(d.id)) else {
            continue;
        };
        let forked = d.fork != 0
            && kept.iter().any(|o| {
                let od = powers::def_of(o.power);
                od.tree == d.tree && od.fork == d.fork
            });
        let known = kept.iter().any(|o| o.power == p.power);
        let tree = d.tree == Tree::Common || d.tree == mine;
        if tree && !known && !forked && kept.len() < hw_net::protocol::MAX_POWERS {
            kept.push(*p);
        }
    }
    s.powers = kept;
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
        assert_eq!(karma_to_next(5), 1920);
        assert_eq!(karma_to_next(6), 2246);
        assert!((1..MAX_LEVEL).all(|l| karma_to_next(l + 1) > karma_to_next(l)));
        let total = karma_for_level(MAX_LEVEL);
        assert!((75_000..80_000).contains(&total), "{total}");
    }

    const ATOMIC: PowerId = PowerId(10);
    const CUFFS: PowerId = PowerId(13);
    const SONIC: PowerId = PowerId(17);
    const BURST: PowerId = PowerId(12);

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn prices_go_by_row_and_powers_owned() {
        let mut s = new_hero(0, Element::Fire);
        assert_eq!(price(&s, ATOMIC), karma_to_next(1));
        assert_eq!(price(&s, CUFFS) as f64, ROW1_PRICE * 1.5);
        s.karma = 100_000;
        buy(&mut s, ATOMIC).unwrap();
        assert_eq!(price(&s, CUFFS) as f64, ROW1_PRICE * 1.5 * 1.25);
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn a_power_needs_what_its_arrows_come_from() {
        let mut s = new_hero(0, Element::Fire);
        s.karma = 100_000;
        assert_eq!(can_buy(&s, CUFFS), Err(BuyError::Needs(ATOMIC)));
        assert_eq!(can_buy(&s, PowerId(55)), Err(BuyError::OtherTree));
        assert_eq!(can_buy(&s, powers::STRIKE), Err(BuyError::NotForSale));
        buy(&mut s, ATOMIC).unwrap();
        buy(&mut s, CUFFS).unwrap();
        // Sonic Boom needs Ray Burst and the cuffs.
        assert_eq!(can_buy(&s, SONIC), Err(BuyError::Needs(BURST)));
        buy(&mut s, BURST).unwrap();
        buy(&mut s, SONIC).unwrap();
        // Travel powers are everyone's; Hero's Return needs two of them.
        assert_eq!(can_buy(&s, PowerId(8)), Err(BuyError::NeedsMore(2)));
        buy(&mut s, PowerId(2)).unwrap();
        buy(&mut s, PowerId(5)).unwrap();
        assert!(can_buy(&s, PowerId(8)).is_ok());
        // Wall Run needs Super Jump or Super Speed.
        assert!(can_buy(&s, PowerId(7)).is_ok());
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn a_fork_is_for_good() {
        let mut s = new_hero(2, Element::Frost); // Born Mighty
        s.karma = 1_000_000;
        for id in [56, 60, 61, 66, 70] {
            buy(&mut s, PowerId(id)).unwrap();
        }
        assert_eq!(
            can_buy(&s, PowerId(71)),
            Err(BuyError::ForkTaken(PowerId(70)))
        );
        // Steel Hide needs either side of that fork.
        assert!(can_buy(&s, PowerId(74)).is_ok());
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn giving_up_returns_ninety_percent_but_not_what_others_need() {
        let mut s = new_hero(0, Element::Fire);
        s.karma = 100_000;
        let cost = buy(&mut s, ATOMIC).unwrap();
        buy(&mut s, CUFFS).unwrap();
        assert_eq!(give_up(&mut s, ATOMIC), Err(GiveUpError::Needed(CUFFS)));
        give_up(&mut s, CUFFS).unwrap();
        let before = s.karma;
        let back = give_up(&mut s, ATOMIC).unwrap();
        assert_eq!(back, (cost as f64 * 0.9) as u32);
        assert_eq!(s.karma, before + back);
        assert_eq!(give_up(&mut s, ATOMIC), Err(GiveUpError::NotOwned));
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn about_thirteen_powers_by_level_twenty() {
        // Buying the cheapest open power whenever karma allows, all the way
        // to level 20.
        let mut s = new_hero(0, Element::Fire);
        s.karma = karma_for_level(MAX_LEVEL) + START_KARMA;
        loop {
            let next = powers::POWERS
                .iter()
                .filter(|d| open(&s, d.id).is_ok())
                .min_by_key(|d| (price(&s, d.id), d.id));
            match next {
                Some(d) if price(&s, d.id) <= s.karma => {
                    buy(&mut s, d.id).unwrap();
                }
                _ => break,
            }
        }
        let n = s.powers.len();
        assert!((11..=16).contains(&n), "{n} powers");
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn refund_all_gives_everything_back() {
        let mut s = new_hero(0, Element::Fire);
        s.karma = 5000;
        buy(&mut s, ATOMIC).unwrap();
        refund_all(&mut s);
        assert_eq!((s.karma, s.powers.len()), (5000, 0));
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
        let p = |n, paid| OwnedPower {
            power: PowerId(n),
            paid,
        };
        let s = sanitize(Sheet {
            origin: 99,
            element: 77,
            // A Masked Avenger: Strike, a double, both sides of a fork,
            // another tree's power, an unknown one.
            powers: vec![
                p(100, 1),
                p(100, 1),
                p(1, 1),
                p(113, 1),
                p(114, 1),
                p(10, 1),
                p(125, 1),
            ],
            ..Default::default()
        });
        assert_eq!(s.origin, 4);
        assert_eq!(s.element, 0);
        assert_eq!(s.powers, vec![p(100, 1), p(113, 1)]);
    }
}
