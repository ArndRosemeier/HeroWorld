//! Making up a case: how many steps, which job at each, and where.
//!
//! A case is a short chain. The last step is the job Annie named (take a
//! boss down, find the evidence, free the hostages); the ones before it
//! are leads: shake down a crew, or search a place, and what turns up
//! points to the next place. Each step picks a kind of place the gang
//! likes ([`super::Gang::haunts`]) that takes its job
//! ([`super::CaseKind::fits`]), then asks that kind's finder for one
//! ([`super::sites::SOURCES`]).
//!
//! **Hook for new jobs.** A new job is a [`CaseKind`] value: say where it
//! fits, whether it can be a lead ([`LEADS`]) or a last step
//! ([`FINALS`]), how its gang stands and when it is done (`Running` in
//! `running.rs`), and its words on the client (missionview, casefile).
//!
//! A plan is a pure function of the world, the world's seed, the case's
//! number, its gang and its variant; the variant counts the plans tried
//! until one kept clear of the open cases. A saved case keeps only these
//! and is planned again on loading.

use super::sites::{self, Place, Places, Site};
use super::{CaseKind, GANGS};
use hw_core::Seed;

/// One step: a job at a place.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub job: CaseKind,
    pub site: Site,
}

/// A whole case.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub gang: usize,
    pub variant: u8,
    pub steps: Vec<Step>,
    /// What the gang is up to (a hideout's crime).
    pub crime: super::board::CrimeKind,
    /// Levels the gangsters stand over the team.
    pub extra: u8,
    /// How big the hideout is: 0 small, 1 middling, 2 big.
    pub size: u8,
    /// A hot tip: it pays more.
    pub hot: bool,
}

/// The case for crime `number` against `gang`: one step in its hideout,
/// or what its shape asks ([`super::board::Shape`]): a heist at a shop
/// front, a getaway over a main road, hostages on a roof, a bagman to
/// tail and then the hideout. A shape the town has no place for is a
/// hideout to bust.
pub fn crime_plan(places: &Places, seed: u64, number: u16, gang: usize) -> Option<Plan> {
    use super::board::Shape;
    let c = super::board::crime(places, seed, number)?;
    let mut rng = Seed(seed).child("crime site").child(number as u64).rng();
    let hideout = sites::building_site(places, c.building);
    let mut find = |place: Place| (0..4).find_map(|_| sites::find(places, place, &mut rng));
    let one = |job: CaseKind, site: Site| vec![Step { job, site }];
    let made: Option<(Vec<Step>, u8)> = match c.shape {
        Shape::Bust => None,
        Shape::Heist => find(Place::Shopfront).map(|s| (one(CaseKind::Heist, s), 1)),
        Shape::Getaway => find(Place::Road).map(|s| (one(CaseKind::Chase, s), 0)),
        Shape::Rooftop => find(Place::Rooftop).map(|s| (one(CaseKind::Hostages, s), 1)),
        Shape::Tail => sites::tail_start(places, &hideout, &mut rng).map(|s| {
            (
                vec![
                    Step {
                        job: CaseKind::Tail,
                        site: s,
                    },
                    Step {
                        job: c.job,
                        site: hideout.clone(),
                    },
                ],
                c.size,
            )
        }),
    };
    let (steps, size) = made.unwrap_or_else(|| (one(c.job, hideout), c.size));
    Some(Plan {
        gang: gang.min(GANGS.len() - 1),
        variant: 0,
        steps,
        crime: c.kind,
        extra: c.extra,
        size,
        hot: c.hot,
    })
}

/// Jobs a lead step can be, and a last step.
pub const LEADS: [CaseKind; 2] = [CaseKind::Crew, CaseKind::Evidence];
pub const FINALS: [CaseKind; 3] = [CaseKind::Takeover, CaseKind::Evidence, CaseKind::Hostages];
/// Steps in a case: one, two or three, two the most often.
const LENGTHS: [usize; 6] = [1, 2, 2, 2, 3, 3];
/// Plans tried for one case before Annie has nothing.
pub const VARIANTS: u8 = 12;

/// Plan `variant` of case `number` for `gang`, or None when the town has
/// no place for its last step.
pub fn plan(places: &Places, seed: u64, number: u16, gang: usize, variant: u8) -> Option<Plan> {
    let mut rng = Seed(seed)
        .child("case")
        .child(number as u64)
        .child(gang as u64)
        .child(variant as u64)
        .rng();
    let steps = LENGTHS[rng.index(LENGTHS.len())];
    let last = FINALS[rng.index(FINALS.len())];
    let mut jobs: Vec<CaseKind> = (1..steps).map(|_| LEADS[rng.index(LEADS.len())]).collect();
    jobs.push(last);
    let mut out: Vec<Step> = Vec::new();
    for (k, &job) in jobs.iter().enumerate() {
        let is_last = k + 1 == jobs.len();
        match pick(places, &mut rng, gang, job, is_last, &out) {
            Some(site) => out.push(Step { job, site }),
            None if is_last => return None,
            None => {}
        }
    }
    Some(Plan {
        gang,
        variant,
        steps: out,
        crime: Default::default(),
        extra: 0,
        size: 2,
        hot: false,
    })
}

/// The first plan of case `number` that keeps clear of `taken` (the open
/// cases' places), and its variant.
pub fn plan_clear(
    places: &Places,
    seed: u64,
    number: u16,
    gang: usize,
    taken: &[Site],
) -> Option<Plan> {
    (0..VARIANTS).find_map(|v| {
        plan(places, seed, number, gang, v).filter(|p| {
            p.steps
                .iter()
                .all(|s| taken.iter().all(|t| !s.site.clashes(t)))
        })
    })
}

/// A place for `job` that `gang` likes, not one an earlier step used. A
/// lead is quick: never a whole building's floors.
fn pick(
    places: &Places,
    rng: &mut hw_core::Rng,
    gang: usize,
    job: CaseKind,
    last: bool,
    before: &[Step],
) -> Option<Site> {
    let mut kinds: Vec<(Place, f64)> = GANGS[gang]
        .haunts
        .iter()
        .copied()
        .filter(|(p, w)| *w > 0.0 && job.fits(*p) && (last || *p != Place::Building))
        .collect();
    while !kinds.is_empty() {
        let w: Vec<f64> = kinds.iter().map(|k| k.1).collect();
        let (place, _) = kinds.remove(rng.weighted(&w));
        for _ in 0..3 {
            if let Some(site) = sites::find(places, place, rng) {
                if before.iter().all(|s| !s.site.clashes(&site)) {
                    return Some(site);
                }
            }
        }
    }
    None
}
