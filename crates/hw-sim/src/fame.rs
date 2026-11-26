//! Fame: how well the town knows a hero, and what its people do about it.
//!
//! A new hero is a nobody (Arnd, 2026-11-08): nobody looks twice. Deeds
//! make a name: fame follows the hero's level (all the karma they ever
//! earned is what the town has seen them do) as long as the townsfolk
//! think well of them. It shows in the street, not in a number: heads
//! turn, people wave and clap, the papers print what the hero did, fans
//! dress in the hero's colours and, for the best known, queue for an
//! autograph.
//!
//! The rules live here; the client draws them (`hw-client/src/fameview.rs`).
//! Nothing here changes what a fight does: fame is how the world answers.

use crate::factions::{self, Attitude};
use crate::progress;
use hw_net::{Faction, Sheet};

/// How well known a hero is, from nobody to legend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Fame {
    /// Nobody looks twice.
    Nobody,
    /// The street talks: a few people know the face and wave.
    Noticed,
    /// A local hero: people stop and clap, the first fans wear the colours.
    Local,
    /// The town's favourite: cheers, cameras, autograph hunters.
    Favourite,
    /// A legend: the whole street turns out.
    Legend,
}

/// The level each step of fame opens at (index: [`Fame::rank`]).
pub const LEVELS: [u8; 5] = [1, 3, 6, 10, 15];

impl Fame {
    pub const ALL: [Fame; 5] = [
        Fame::Nobody,
        Fame::Noticed,
        Fame::Local,
        Fame::Favourite,
        Fame::Legend,
    ];

    /// 0 (nobody) to 4 (legend).
    pub fn rank(self) -> u8 {
        self as u8
    }

    pub fn from_rank(rank: u8) -> Self {
        Self::ALL[(rank as usize).min(Self::ALL.len() - 1)]
    }

    /// The fame a hero of `level` has when the town likes them.
    pub fn of_level(level: u8) -> Self {
        let rank = LEVELS.iter().rposition(|l| level >= *l).unwrap_or(0);
        Self::from_rank(rank as u8)
    }

    /// How well the town knows the hero of this sheet. A hero the
    /// townsfolk do not think well of (less than friendly) is known, but
    /// nobody cheers: they count as a nobody here.
    pub fn of(sheet: &Sheet) -> Self {
        let liked = Attitude::of(factions::toward_hero(Faction::Townsfolk, &sheet.regard))
            >= Attitude::Friendly;
        if !liked {
            return Fame::Nobody;
        }
        Self::of_level(progress::level_of(sheet.lifetime))
    }

    /// What the town calls a hero this well known.
    pub fn title(self) -> &'static str {
        match self {
            Fame::Nobody => "Unknown",
            Fame::Noticed => "Talk of the street",
            Fame::Local => "Local hero",
            Fame::Favourite => "The town's favourite",
            Fame::Legend => "Living legend",
        }
    }

    /// The level the next step opens at, if there is one.
    pub fn next_level(self) -> Option<u8> {
        LEVELS.get(self.rank() as usize + 1).copied()
    }

    /// The share of passers-by who stop for the hero when they meet.
    pub fn admirers(self) -> f64 {
        match self {
            Fame::Nobody => 0.0,
            Fame::Noticed => 0.12,
            Fame::Local => 0.3,
            Fame::Favourite => 0.5,
            Fame::Legend => 0.75,
        }
    }

    /// The share of the town's people who go about in the hero's colours.
    pub fn fans(self) -> f64 {
        match self {
            Fame::Nobody | Fame::Noticed => 0.0,
            Fame::Local => 0.03,
            Fame::Favourite => 0.07,
            Fame::Legend => 0.12,
        }
    }

    /// The share of those who stop that ask for an autograph.
    pub fn autographs(self) -> f64 {
        match self {
            Fame::Favourite => 0.2,
            Fame::Legend => 0.3,
            _ => 0.0,
        }
    }

    /// The papers print the hero's name (before that it is "a masked
    /// stranger" who stopped the robbery).
    pub fn named_in_print(self) -> bool {
        self >= Fame::Local
    }
}

/// What someone who stops for a hero does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Greeting {
    /// A wave in passing.
    Wave,
    /// Clapping.
    Clap,
    /// Both arms up, bouncing on the toes.
    Cheer,
    /// A little book and a pencil held out: sign it by standing with them.
    Autograph,
}

/// People stop for a hero who comes within this distance (m)...
pub const NOTICE: f64 = 9.0;
/// ...and go on their way when the hero is this far off again.
pub const LOSE: f64 = 15.0;
/// A hero faster than this (m/s) is gone before anyone can stop: sprinting
/// past, or flying low over the street.
pub const TOO_FAST: f64 = 7.5;
/// How long one of them stays (s), at least and at most.
pub const STAY: (f64, f64) = (3.0, 6.5);
/// How long someone waits with their autograph book (s).
pub const AUTOGRAPH_WAIT: f64 = 14.0;
/// Seconds before the same person stops for a hero again.
pub const AGAIN: f64 = 240.0;
/// A hero stands this close (m) and this still (m/s) to sign...
pub const SIGN_REACH: f64 = 2.0;
pub const SIGN_STILL: f64 = 0.6;
/// ...and signing takes this long (s).
pub const SIGN_SECONDS: f64 = 2.4;

/// Is `citizen` one of the hero's fans, dressed in the colours? The same
/// people every time (by their number and the town's seed), more of them
/// the better known the hero is: a fan at "local hero" stays one.
pub fn is_fan(seed: hw_core::Seed, citizen: u64, fame: Fame) -> bool {
    seed.child("fan").child(citizen).unit() < fame.fans()
}

/// What `citizen` does on meeting a hero of `fame` at the encounter
/// numbered `meeting` (a count of [`AGAIN`] slots, so the same person
/// answers the same way on every machine): nothing for most, at first.
/// Fans always stop.
pub fn greeting(seed: hw_core::Seed, citizen: u64, meeting: u64, fame: Fame) -> Option<Greeting> {
    if fame == Fame::Nobody {
        return None;
    }
    let s = seed.child("greeting").child(citizen).child(meeting);
    if !is_fan(seed, citizen, fame) && s.child("stops").unit() >= fame.admirers() {
        return None;
    }
    if s.child("autograph").unit() < fame.autographs() {
        return Some(Greeting::Autograph);
    }
    let x = s.child("how").unit();
    Some(match fame {
        Fame::Nobody => return None,
        Fame::Noticed => Greeting::Wave,
        Fame::Local => {
            if x < 0.6 {
                Greeting::Wave
            } else {
                Greeting::Clap
            }
        }
        Fame::Favourite | Fame::Legend => {
            if x < 0.3 {
                Greeting::Wave
            } else if x < 0.6 {
                Greeting::Clap
            } else {
                Greeting::Cheer
            }
        }
    })
}

/// How long `citizen` stays at that meeting (s).
pub fn stay(seed: hw_core::Seed, citizen: u64, meeting: u64) -> f64 {
    let x = seed
        .child("greeting")
        .child(citizen)
        .child(meeting)
        .child("stay")
        .unit();
    STAY.0 + (STAY.1 - STAY.0) * x
}

#[cfg(test)]
mod tests {
    use super::*;
    use hw_core::Seed;

    fn sheet(level: u8) -> Sheet {
        Sheet {
            lifetime: progress::karma_for_level(level),
            ..Sheet::default()
        }
    }

    #[test]
    fn a_new_hero_is_a_nobody_and_deeds_make_a_name() {
        assert_eq!(Fame::of(&sheet(1)), Fame::Nobody);
        assert_eq!(Fame::of(&sheet(2)), Fame::Nobody);
        assert_eq!(Fame::of(&sheet(3)), Fame::Noticed);
        assert_eq!(Fame::of(&sheet(6)), Fame::Local);
        assert_eq!(Fame::of(&sheet(10)), Fame::Favourite);
        assert_eq!(Fame::of(&sheet(progress::MAX_LEVEL)), Fame::Legend);
        // Never down as the level goes up.
        let mut last = Fame::Nobody;
        for level in 1..=progress::MAX_LEVEL {
            let f = Fame::of(&sheet(level));
            assert!(f >= last);
            last = f;
        }
    }

    #[test]
    fn a_hero_the_town_dislikes_gets_no_cheers() {
        let mut s = sheet(progress::MAX_LEVEL);
        s.regard[Faction::Townsfolk.index()] = -60;
        assert_eq!(Fame::of(&s), Fame::Nobody);
    }

    #[test]
    fn nobody_stops_for_a_nobody() {
        let seed = Seed(7);
        for id in 0..2000 {
            assert_eq!(greeting(seed, id, 0, Fame::Nobody), None);
            assert!(!is_fan(seed, id, Fame::Nobody));
        }
    }

    #[test]
    fn more_people_stop_the_better_known_the_hero_is() {
        let seed = Seed(11);
        let stops = |f: Fame| {
            (0..4000)
                .filter(|id| greeting(seed, *id, 3, f).is_some())
                .count()
        };
        let n: Vec<usize> = Fame::ALL.iter().map(|f| stops(*f)).collect();
        assert!(n.windows(2).all(|w| w[0] < w[1]), "{n:?}");
        // A talk of the street: about one in eight; a legend: most.
        assert!((350..650).contains(&n[1]), "{n:?}");
        assert!(n[4] > 2800, "{n:?}");
    }

    #[test]
    fn fans_stay_fans_and_always_stop() {
        let seed = Seed(5);
        let mut fans = 0;
        for id in 0..5000 {
            if is_fan(seed, id, Fame::Local) {
                fans += 1;
                assert!(is_fan(seed, id, Fame::Favourite));
                assert!(is_fan(seed, id, Fame::Legend));
                for meeting in 0..4 {
                    assert!(greeting(seed, id, meeting, Fame::Local).is_some());
                }
            }
        }
        assert!((90..220).contains(&fans), "{fans}");
    }

    #[test]
    fn only_the_well_known_are_asked_for_autographs() {
        let seed = Seed(3);
        let asks = |f: Fame| {
            (0..3000)
                .filter(|id| greeting(seed, *id, 1, f) == Some(Greeting::Autograph))
                .count()
        };
        assert_eq!(asks(Fame::Noticed), 0);
        assert_eq!(asks(Fame::Local), 0);
        assert!(asks(Fame::Favourite) > 100);
        assert!(asks(Fame::Legend) > asks(Fame::Favourite));
    }

    #[test]
    fn the_same_person_answers_the_same_on_every_machine() {
        let seed = Seed(99);
        for id in 0..200 {
            assert_eq!(
                greeting(seed, id, 5, Fame::Favourite),
                greeting(seed, id, 5, Fame::Favourite)
            );
            let s = stay(seed, id, 5);
            assert!((STAY.0..=STAY.1).contains(&s));
        }
    }
}
