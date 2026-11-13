//! The police computer's crimes (Crime-o-Vision): gang hideouts to bust.
//!
//! Crime number `n` of a world is a pure function of the world's seed and
//! `n` ([`crime`]): which hideout (the town's hideouts in a seeded order),
//! what the gang is up to ([`CrimeKind`]), the job (arrest the boss, or
//! find the thing) and how many levels over the team the gangsters stand.
//! Which gang it is depends on the team's level when it is looked at
//! ([`gang_for`]), and is kept with the case once taken.
//!
//! The board lists the next [`BOARD`] crimes after the ones closed; a hero
//! takes one from the computer and a case opens on it.

use super::sites::Places;
use super::{CaseKind, GANGS};
use hw_core::Seed;

/// Crimes on the board at once.
pub const BOARD: usize = 10;

/// A crime stays on the board about this long (s): from half to one and a
/// half times it, by its number; a hot one half of that.
pub const LIFE_SECONDS: f64 = 20.0 * 60.0;
/// One crime in this many is a hot tip, and what it pays on top.
pub const HOT_CHANCE: f64 = 1.0 / 6.0;
pub const HOT_BONUS: f64 = 0.5;
/// A crime nobody took makes the streets round its hideout rough for this
/// long (s): street crime is as common there as round a gang's hangout.
pub const EMBOLDENED_SECONDS: f64 = 10.0 * 60.0;

/// How long crime `number` stays on the board (s).
pub fn life(seed: u64, number: u16, hot: bool) -> f64 {
    let f = Seed(seed).child("life").child(number as u64).rng().unit();
    LIFE_SECONDS * (0.5 + f) * if hot { 0.5 } else { 1.0 }
}

/// What the gang is up to in its hideout: it decides the symbol, the words
/// and the thing to find.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CrimeKind {
    /// A robbed payroll, the cash piled up.
    Payroll,
    /// A counterfeiting press.
    Counterfeit,
    /// Contraband, crated up.
    Smuggling,
    /// A stolen prototype from a laboratory.
    Prototype,
    /// Stolen art.
    Art,
    /// A protection racket's ledger.
    #[default]
    Racket,
}

impl CrimeKind {
    pub const ALL: [CrimeKind; 6] = [
        CrimeKind::Payroll,
        CrimeKind::Counterfeit,
        CrimeKind::Smuggling,
        CrimeKind::Prototype,
        CrimeKind::Art,
        CrimeKind::Racket,
    ];

    pub fn code(self) -> u8 {
        CrimeKind::ALL.iter().position(|k| *k == self).unwrap_or(0) as u8
    }

    pub fn from_code(c: u8) -> Self {
        CrimeKind::ALL.get(c as usize).copied().unwrap_or_default()
    }

    /// The police's name for it.
    pub fn title(self) -> &'static str {
        match self {
            CrimeKind::Payroll => "Payroll robbery",
            CrimeKind::Counterfeit => "Counterfeit ring",
            CrimeKind::Smuggling => "Smuggling den",
            CrimeKind::Prototype => "Stolen prototype",
            CrimeKind::Art => "Art theft",
            CrimeKind::Racket => "Protection racket",
        }
    }

    /// The thing to find, for "find the ...".
    pub fn thing(self) -> &'static str {
        match self {
            CrimeKind::Payroll => "the payroll strongbox",
            CrimeKind::Counterfeit => "the printing plates",
            CrimeKind::Smuggling => "the contraband crate",
            CrimeKind::Prototype => "the stolen prototype",
            CrimeKind::Art => "the stolen painting",
            CrimeKind::Racket => "the racket's ledger",
        }
    }
}

/// Crime number `n`, planned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Crime {
    pub number: u16,
    pub kind: CrimeKind,
    pub job: CaseKind,
    /// Levels the gangsters stand over the team (0 to 2).
    pub extra: u8,
    /// How big the hideout is: 0 small, 1 middling, 2 big (never bigger
    /// than its building has rooms for).
    pub size: u8,
    /// A hot tip: it pays [`HOT_BONUS`] more and is gone twice as soon.
    pub hot: bool,
    /// The hideout, by index in the route maps ([`Places::floors`]).
    pub building: usize,
}

/// Crime `number` (counting from 1), if the town has a hideout.
pub fn crime(places: &Places, seed: u64, number: u16) -> Option<Crime> {
    let n = places.hideouts.len();
    if n == 0 || number == 0 {
        return None;
    }
    let mut rng = Seed(seed).child("crime").child(number as u64).rng();
    let kind = CrimeKind::ALL[rng.index(CrimeKind::ALL.len())];
    let job = if rng.chance(0.5) {
        CaseKind::Takeover
    } else {
        CaseKind::Evidence
    };
    let extra = rng.index(3) as u8;
    let building = places.hideouts[(number as usize - 1) % n];
    // Besides the boss's room.
    let spare = places
        .rooms
        .get(building)
        .map_or(0, |r| r.saturating_sub(1)) as usize;
    let fits = (0..3u8)
        .rev()
        .find(|&s| s == 0 || spare >= super::rooms_of(s) - 1)
        .unwrap_or(0);
    Some(Crime {
        number,
        kind,
        job,
        extra,
        size: (rng.index(3) as u8).min(fits),
        hot: rng.chance(HOT_CHANCE),
        building,
    })
}

/// The gang behind crime `number` when the team is at level `team`: one
/// that comes out at that level.
pub fn gang_for(seed: u64, number: u16, team: u8) -> usize {
    let gangs: Vec<usize> = (0..GANGS.len())
        .filter(|&g| GANGS[g].level <= team)
        .collect();
    let mut rng = Seed(seed).child("gang").child(number as u64).rng();
    gangs[rng.index(gangs.len())]
}
