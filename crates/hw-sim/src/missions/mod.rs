//! Cases: missions made up from the town. Atomic Annie at Hero Hall hands
//! them out: a hero presses the help key (E) beside her and gets one.
//!
//! A case is a short chain of steps ([`plan`]): leads first (shake down a
//! crew, search a place), then the job (take a boss down, find the
//! evidence, free the hostages). Each step plays at a place of the town
//! ([`sites`]): a building with a floor plan, a street, a landmark's plaza,
//! the utility tunnels, an Aether Tube station, a grotto of the Deeps, the
//! buried city, the Doctor's lab. The radio leads to it (under the ground:
//! to the way down first); once a hero comes near, the gang stands there.
//!
//! Hooks: new kinds of place go in [`sites::SOURCES`], new jobs in
//! [`CaseKind`] (see [`plan`]); a gang's taste in places is
//! [`Gang::haunts`]. Steps, the radio, the case file and saving all work
//! from those.
//!
//! A group notices a hero only as it always does: in sight and in reach.
//! In a building, walls hide the rooms from each other, so a careful hero
//! clears them one at a time (Arnd, 2026-11-11: "not overly aware").
//! Villains who did notice follow through doorways and up the stairs by
//! the building's route map ([`crate::floornav`]).
//!
//! Several cases can be open at once ([`OPEN_LIMIT`], the only place that
//! says how many), never two at one place, each with its own number.
//! Done: karma for the heroes near (a share of their next level; a little
//! for each lead). The open cases and the counts are kept in the host's
//! hero file ([`hw_net::CaseBook`]) and planned again on the next start.
//! Leave a place for long and the gang goes; come back and it stands
//! again, fresh. Cases are made up from the world's seed and a running
//! count: the same town gives the same cases.

pub mod plan;
mod running;
pub mod sites;

pub use plan::{Plan, Step};
pub use running::Running;
pub use sites::{Place, Places, Site};

use crate::combat::{self, Fights};
use crate::floornav::Floors;
use crate::{progress, Control, Fighter, Hero, Motion, Net};
use bevy_ecs::prelude::*;
use glam::DVec3;
use hw_core::{NetId, Seed};
use hw_gen::urban::area::Area;
use hw_gen::urban::landmarks::LandmarkKind;
use hw_gen::urban::world::TownWorld;
use hw_net::protocol::MAX_SAVED_CASES;
use hw_net::{CaseBook, SavedCase, ToClient};
use std::collections::BTreeMap;
use std::sync::mpsc::Receiver;
use std::sync::Arc;

/// Who hands the cases out (one of Hero Hall's heroes, by index; the
/// story's Commander Starling is 0).
pub const CONTACT: usize = 1;
pub const CONTACT_NAME: &str = crate::npcheroes::NAMES[CONTACT];
/// A hero holding the help key this near her takes a case (m).
pub const ASK_REACH: f64 = 3.5;
/// The gang takes its rooms once a hero is this near the building (m).
pub const WAKE: f64 = 90.0;
/// No hero this near for [`AWAY_SECONDS`]: the gang leaves (m).
pub const AWAY: f64 = 250.0;
pub const AWAY_SECONDS: f64 = 30.0;
/// Rooms the gang holds besides the boss's.
pub const ROOMS: usize = 6;
/// Clues an evidence case hides.
pub const CLUES: usize = 3;
/// A hero this near a clue can search it (m), for this long (s).
pub const CLUE_REACH: f64 = 2.8;
pub const CLUE_SECONDS: f64 = 3.0;
/// Hostages a hostage case holds, and the seconds it takes to untie one.
pub const HOSTAGES: usize = 3;
pub const FREE_SECONDS: f64 = 2.0;
/// A clue's or hostage's state, as [`ToClient::Case`] carries it: still
/// guarded by its room's crew, ready (search it, untie them), done.
pub const GUARDED: u8 = 0;
pub const READY: u8 = 1;
pub const DONE: u8 = 2;
/// Bystander numbers of hostages (apart from incidents' and street
/// crimes'): this bit, the case's number and the hostage's.
pub(crate) const HOSTAGE_IDS: u32 = 0x4000_0000;
/// Heroes this near the building share the reward (m) ...
pub const REWARD: f64 = 150.0;
/// ... this share of what their next level takes.
pub const REWARD_SHARE: f64 = 0.4;
/// Villains in a room for one hero (one more for each friend), and with
/// the boss.
pub const ROOM_SIZE: u32 = 3;
pub const BOSS_SIZE: u32 = 3;
/// The boss and its guard stand this many levels over the team.
pub const BOSS_EXTRA: u8 = 1;

/// Stages, as [`ToClient::Case`] carries them.
pub const IDLE: u8 = 0;
pub const GO: u8 = 1;
pub const INSIDE: u8 = 2;
/// Sent once, as a case ends.
pub const CLOSED: u8 = 3;
/// Cases open at once, at most (and never two at one place).
pub const OPEN_LIMIT: usize = 3;
/// Under the ground the gang stands once a hero is this near (m, in a
/// straight line: a hero on the street above is not near).
pub const UNDER_WAKE: f64 = 70.0;
/// Heroes near share this much of their next level for a lead step.
pub const LEAD_SHARE: f64 = 0.1;
/// Groups out in the open: a crew of this many, and side groups.
pub const OPEN_SIZE: u32 = 3;
/// A case is planned on a worker thread and opens this many ticks after it
/// was asked for (or saved), whatever the worker's speed: the server
/// waits for the plan at that tick if it is not there yet.
pub const PLAN_TICKS: u64 = 2 * crate::TICK_HZ as u64;
/// A case opened by hand (tests, screenshots): its variant, never saved.
pub const FORCED: u8 = 255;

/// A job at a place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseKind {
    /// Beat the boss (in a building: the gang holds its rooms too).
    Takeover,
    /// Find the clues (hold E at each).
    Evidence,
    /// Beat the guards, then untie the hostages (hold E).
    Hostages,
    /// Beat the crew standing there (a lead: they talk).
    Crew,
}

impl CaseKind {
    pub const ALL: [CaseKind; 4] = [
        CaseKind::Takeover,
        CaseKind::Evidence,
        CaseKind::Hostages,
        CaseKind::Crew,
    ];

    pub fn code(self) -> u8 {
        CaseKind::ALL.iter().position(|k| *k == self).unwrap_or(0) as u8
    }

    pub fn from_code(c: u8) -> Self {
        CaseKind::ALL
            .get(c as usize)
            .copied()
            .unwrap_or(CaseKind::Takeover)
    }

    /// Can this job play at `place`? (A crew on a street corner, not in a
    /// building's rooms.)
    pub fn fits(self, place: Place) -> bool {
        match self {
            CaseKind::Crew => place != Place::Building,
            _ => true,
        }
    }
}

/// A gang that takes cases: its people, its lieutenant, the team level
/// it comes out at and the kinds of place it likes (weights; a place
/// missing here it never uses).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gang {
    pub name: &'static str,
    /// Who fights up close and who shoots (None: the Doctor's own mix).
    pub crew: Option<(EnemyKind, EnemyKind)>,
    pub boss: EnemyKind,
    pub level: u8,
    pub haunts: &'static [(Place, f64)],
}

use crate::combat::EnemyKind;

pub const GANGS: [Gang; 5] = [
    Gang {
        name: "the Doctor's technicians",
        crew: None,
        boss: EnemyKind::Sentry,
        level: 1,
        haunts: &[
            (Place::Building, 2.0),
            (Place::Street, 2.0),
            (Place::Tunnel, 3.0),
            (Place::Station, 1.0),
            (Place::Plaza, 1.0),
            (Place::Lair, 2.0),
        ],
    },
    Gang {
        name: "the Chrome Jackets and the Velvet Syndicate",
        crew: Some((EnemyKind::Greaser, EnemyKind::Gunsel)),
        boss: EnemyKind::Alpha,
        level: 1,
        haunts: &[
            (Place::Building, 3.0),
            (Place::Street, 4.0),
            (Place::Station, 2.0),
            (Place::Ruin, 2.0),
            (Place::Tunnel, 1.0),
            (Place::Plaza, 1.0),
        ],
    },
    Gang {
        name: "the sky pirates",
        crew: Some((EnemyKind::Buccaneer, EnemyKind::Gunner)),
        boss: EnemyKind::Captain,
        level: 2,
        haunts: &[
            (Place::Building, 2.0),
            (Place::Plaza, 3.0),
            (Place::Street, 2.0),
            (Place::Station, 1.0),
            (Place::Ruin, 1.0),
        ],
    },
    Gang {
        name: "the Iron-Masked Empire",
        crew: Some((EnemyKind::Legionary, EnemyKind::Legionary)),
        boss: EnemyKind::Praetorian,
        level: crate::incidents::IRON_LEVEL,
        haunts: &[
            (Place::Building, 3.0),
            (Place::Street, 3.0),
            (Place::Plaza, 2.0),
            (Place::Tunnel, 2.0),
        ],
    },
    Gang {
        name: "the invaders from Zorrak",
        crew: Some((EnemyKind::Grabber, EnemyKind::Zapper)),
        boss: EnemyKind::Overlord,
        level: crate::incidents::SAUCER_LEVEL,
        haunts: &[
            (Place::Plaza, 3.0),
            (Place::Street, 2.0),
            (Place::Grotto, 3.0),
            (Place::Building, 2.0),
            (Place::Ruin, 1.0),
        ],
    },
];

/// A case being planned on a worker thread.
struct Pending {
    number: u16,
    gang: usize,
    /// A saved case's plan (else the first that keeps clear of the open
    /// cases: Annie's new one).
    variant: Option<u8>,
    step: usize,
    /// The tick it opens at.
    due: u64,
    plan: Receiver<Option<Plan>>,
}

/// The cases: those being worked, and Annie's desk. Nothing here assumes
/// one case: [`Missions::limit`] says how many may be open at once (never
/// two at one place), and every case keeps its own groups, clues and
/// hostages.
pub struct Missions {
    places: Option<Arc<Places>>,
    /// Where Annie stands.
    contact: Option<DVec3>,
    seed: u64,
    pub cases: Vec<Running>,
    pending: Vec<Pending>,
    /// Cases closed so far.
    pub closed: u16,
    /// Cases handed out so far (the next one's number is one more).
    pub opened: u16,
    /// Most cases open at once.
    pub limit: usize,
    /// Heroes holding the help key by Annie (a new case takes a new press).
    asking: Vec<NetId>,
    /// Annie found no place for a new case (every one in the town is
    /// taken): she has none until one closes.
    dry: bool,
    last_sent: BTreeMap<u16, ToClient>,
    desk_sent: Option<ToClient>,
    news: Vec<ToClient>,
    /// The book last written into the heroes' files (or taken from the
    /// host's): a host's book that differs was loaded, and is adopted.
    last_book: CaseBook,
}

impl Default for Missions {
    fn default() -> Self {
        Missions {
            places: None,
            contact: None,
            seed: 0,
            cases: Vec::new(),
            pending: Vec::new(),
            closed: 0,
            opened: 0,
            limit: OPEN_LIMIT,
            asking: Vec::new(),
            dry: false,
            last_sent: BTreeMap::new(),
            desk_sent: None,
            news: Vec::new(),
            last_book: CaseBook::default(),
        }
    }
}

/// Hero Hall, where Annie hands the cases out.
fn hall_contact(world: &TownWorld) -> Option<DVec3> {
    world
        .landmarks
        .iter()
        .find(|l| l.kind == LandmarkKind::HeroHall)
        .and_then(|hall| {
            hall.hero_spots()
                .get(CONTACT)
                .map(|(p, _)| DVec3::new(p.x, hall.base, p.y))
        })
}

impl Missions {
    /// Cases in the town `world`, its streets round the start `area` and
    /// the buildings with a route map.
    pub fn plan(world: &Arc<TownWorld>, area: Arc<Area>, floors: Arc<Floors>, seed: u64) -> Self {
        let contact = hall_contact(world);
        let names = world
            .landmarks
            .iter()
            .filter(|l| hw_gen::urban::comet::layout(l).is_some())
            .map(|l| hw_gen::urban::comet::name(l).to_string())
            .collect();
        let keep_clear = contact
            .iter()
            .map(|c| glam::DVec2::new(c.x, c.z))
            .chain(std::iter::once(world.spawn_line().0))
            .collect();
        Missions {
            places: Some(Arc::new(Places {
                town: Some(world.clone()),
                area: Some(area),
                floors,
                names,
                keep_clear,
            })),
            contact,
            seed,
            ..Default::default()
        }
    }

    /// Cases in the buildings of `floors` only, handed out at `contact`
    /// (tests).
    pub fn new(floors: Arc<Floors>, contact: Option<DVec3>, seed: u64) -> Self {
        Missions {
            places: Some(Arc::new(Places {
                floors,
                ..Default::default()
            })),
            contact,
            seed,
            ..Default::default()
        }
    }

    /// The square with people and traffic moved (it follows the heroes
    /// across a big city): street cases go to its streets from now on, and
    /// the old square is let go.
    pub fn set_area(&mut self, area: Arc<Area>) {
        if let Some(p) = &mut self.places {
            if p.area.is_some() {
                let mut next = Places::clone(p);
                next.area = Some(area);
                *p = Arc::new(next);
            }
        }
    }

    /// What the finders look at.
    pub fn places(&self) -> Option<&Arc<Places>> {
        self.places.as_ref()
    }

    /// Opens a one-step case at building `building` now (tests and
    /// screenshots), dropping a case there. Its number.
    pub fn force(
        &mut self,
        world: &mut World,
        kind: CaseKind,
        building: usize,
        gang: usize,
    ) -> u16 {
        let Some(places) = self.places.clone() else {
            return 0;
        };
        let site = sites::building_site(&places, building);
        self.force_steps(world, gang, vec![Step { job: kind, site }])
    }

    /// Opens a one-step case of `kind` at a place of kind `place` now
    /// (screenshots). Its number, or None when the town has no such place.
    pub fn force_at(
        &mut self,
        world: &mut World,
        kind: CaseKind,
        place: Place,
        gang: usize,
    ) -> Option<u16> {
        let places = self.places.clone()?;
        let mut rng = Seed(self.seed)
            .child("forced")
            .child(place.code() as u64)
            .rng();
        let site = (0..6).find_map(|_| sites::find(&places, place, &mut rng))?;
        Some(self.force_steps(world, gang, vec![Step { job: kind, site }]))
    }

    /// Plans a whole case for `gang` right here and opens it (tests and
    /// screenshots). Its number.
    pub fn force_plan(&mut self, world: &mut World, gang: usize) -> Option<u16> {
        let places = self.places.clone()?;
        let number = self.opened.wrapping_add(1);
        let taken = self.taken();
        let plan = plan::plan_clear(&places, self.seed, number, gang, &taken)?;
        let _ = world;
        self.opened = number;
        self.cases
            .push(Running::new(number, plan, 0, places.floors.clone()));
        Some(number)
    }

    fn force_steps(&mut self, world: &mut World, gang: usize, steps: Vec<Step>) -> u16 {
        let Some(places) = self.places.clone() else {
            return 0;
        };
        let clash = |r: &Running| {
            r.steps()
                .iter()
                .any(|a| steps.iter().any(|b| a.site.clashes(&b.site)))
        };
        while let Some(k) = self.cases.iter().position(clash) {
            let mut r = self.cases.remove(k);
            r.leave(world);
            self.dropped(r.number);
        }
        self.opened = self.opened.wrapping_add(1);
        let number = self.opened;
        let plan = Plan {
            gang: gang.min(GANGS.len() - 1),
            variant: FORCED,
            steps,
        };
        self.cases
            .push(Running::new(number, plan, 0, places.floors.clone()));
        number
    }

    /// The case with this number, if open.
    pub fn case(&self, number: u16) -> Option<&Running> {
        self.cases.iter().find(|r| r.number == number)
    }

    /// How many buildings can hold a case.
    pub fn buildings(&self) -> usize {
        self.places.as_ref().map_or(0, |p| p.floors.buildings.len())
    }

    /// Does this town have cases (a contact, and a place to hold them)?
    pub fn available(&self) -> bool {
        self.contact.is_some()
            && self
                .places
                .as_ref()
                .is_some_and(|p| !p.floors.buildings.is_empty() || p.town.is_some())
    }

    /// Can Annie hand out another case now?
    pub fn can_take(&self) -> bool {
        self.available()
            && self.cases.len() + self.pending.len() < self.limit
            && self.pending.iter().all(|p| p.variant.is_some())
            && !self.dry
    }

    /// The places of the open cases (what a new one keeps clear of).
    fn taken(&self) -> Vec<Site> {
        self.cases
            .iter()
            .flat_map(|r| r.steps()[r.step..].iter().map(|s| s.site.clone()))
            .collect()
    }

    /// Plans case `number` on a worker thread; it opens at `tick +`
    /// [`PLAN_TICKS`].
    fn start_plan(
        &mut self,
        tick: u64,
        number: u16,
        gang: usize,
        variant: Option<u8>,
        step: usize,
    ) {
        let Some(places) = self.places.clone() else {
            return;
        };
        let seed = self.seed;
        let taken = self.taken();
        let (tx, rx) = std::sync::mpsc::channel();
        let work = move || {
            let plan = match variant {
                Some(v) => plan::plan(&places, seed, number, gang, v),
                None => plan::plan_clear(&places, seed, number, gang, &taken),
            };
            let _ = tx.send(plan);
        };
        // Planning reads deep tiles and streets: off the server's tick.
        if let Err(e) = std::thread::Builder::new()
            .name("case-plan".into())
            .spawn(work)
        {
            eprintln!("case planner: {e}");
        }
        self.pending.push(Pending {
            number,
            gang,
            variant,
            step,
            due: tick + PLAN_TICKS,
            plan: rx,
        });
    }

    /// Cases whose plan is due open now; the server waits for a plan not
    /// yet done, so a case opens at the same tick on every run.
    fn open_planned(&mut self, tick: u64) {
        let Some(places) = self.places.clone() else {
            return;
        };
        let mut k = 0;
        while k < self.pending.len() {
            if self.pending[k].due > tick {
                k += 1;
                continue;
            }
            let p = self.pending.remove(k);
            let Some(plan) = p.plan.recv().ok().flatten() else {
                self.dry |= p.variant.is_none();
                continue;
            };
            if p.variant.is_none() {
                self.opened = self.opened.max(p.number);
            }
            let step = p.step.min(plan.steps.len().saturating_sub(1));
            let r = Running::new(p.number, plan, step, places.floors.clone());
            let at = self
                .cases
                .iter()
                .position(|c| c.number > r.number)
                .unwrap_or(self.cases.len());
            self.cases.insert(at, r);
            let _ = p.gang;
        }
    }

    /// Every tick: searching at clues and hostages, asking Annie, cases
    /// planned opening. Once a second: gangs standing at their places,
    /// steps and cases done.
    pub(crate) fn step(&mut self, world: &mut World, tick: u64) {
        crate::incidents::walk_ways(world, tick);
        if !self.available() {
            return;
        }
        self.open_planned(tick);
        let heroes = heroes(world);
        if heroes.is_empty() {
            return;
        }
        for r in &mut self.cases {
            r.search(world, tick, &heroes);
        }
        self.ask(world, tick, &heroes);
        if tick.is_multiple_of(crate::TICK_HZ as u64) {
            self.keep_book(world, tick);
            let mut done = Vec::new();
            for (k, r) in self.cases.iter_mut().enumerate() {
                match r.settle(world, tick, &heroes) {
                    running::Settled::Working => {}
                    running::Settled::Lead(at) => {
                        reward(world, &heroes, at, LEAD_SHARE);
                    }
                    running::Settled::Done => done.push(k),
                }
            }
            for k in done.into_iter().rev() {
                let r = self.cases.remove(k);
                self.close(world, &heroes, r);
            }
        }
        self.tell(world);
    }

    /// The cases in the host's file: a book that is new to us (the host
    /// came, a file was loaded) is taken over, open cases and all; then
    /// what we have now is written into every hero's file.
    fn keep_book(&mut self, world: &mut World, tick: u64) {
        let host = world
            .query::<(&Hero, &crate::Progress)>()
            .iter(world)
            .min_by_key(|(h, _)| h.owner)
            .map(|(_, p)| p.sheet.cases.clone());
        if let Some(book) = host.filter(|b| *b != self.last_book) {
            self.adopt(world, tick, &book);
        }
        let book = self.book();
        if book != self.last_book {
            for mut p in world.query::<&mut crate::Progress>().iter_mut(world) {
                if p.sheet.cases != book {
                    p.sheet.cases = book.clone();
                    p.changed = true;
                }
            }
            self.last_book = book;
        }
    }

    /// The cases as a hero file keeps them (cases opened by hand are not
    /// kept).
    pub fn book(&self) -> CaseBook {
        let mut open: Vec<SavedCase> = self
            .cases
            .iter()
            .filter(|r| r.variant() != FORCED)
            .map(|r| SavedCase {
                number: r.number,
                gang: r.gang as u8,
                variant: r.variant(),
                step: r.step.min(255) as u8,
            })
            .chain(self.pending.iter().filter_map(|p| {
                Some(SavedCase {
                    number: p.number,
                    gang: p.gang as u8,
                    variant: p.variant?,
                    step: p.step.min(255) as u8,
                })
            }))
            .collect();
        open.sort_by_key(|c| c.number);
        open.truncate(MAX_SAVED_CASES);
        CaseBook {
            world: self.seed,
            opened: self.opened,
            closed: self.closed,
            open,
        }
    }

    /// Takes over a saved book: its counts, and its open cases if they are
    /// of this world (planned again from what was kept).
    fn adopt(&mut self, world: &mut World, tick: u64, book: &CaseBook) {
        for mut r in std::mem::take(&mut self.cases) {
            r.leave(world);
            self.dropped(r.number);
        }
        self.pending.clear();
        self.opened = book.opened;
        self.closed = book.closed;
        if book.world == self.seed {
            for c in &book.open {
                if (c.gang as usize) < GANGS.len() && c.variant != FORCED {
                    self.start_plan(
                        tick,
                        c.number,
                        c.gang as usize,
                        Some(c.variant),
                        c.step as usize,
                    );
                    self.opened = self.opened.max(c.number);
                }
            }
        }
        self.last_book = book.clone();
    }

    /// A hero pressing the help key by Annie gets a case, if she has one.
    fn ask(&mut self, world: &mut World, tick: u64, heroes: &[Seen]) {
        // Where she stands now (pushed about in a scuffle), else her place.
        let contact = world
            .query::<(&crate::npcheroes::NpcHero, &Motion)>()
            .iter(world)
            .find(|(n, _)| n.index as usize == CONTACT)
            .map_or(self.contact.expect("available"), |(_, m)| m.0.pos);
        let holding: Vec<NetId> = heroes
            .iter()
            .filter(|h| h.help && h.at.distance(contact) < ASK_REACH)
            .map(|h| h.id)
            .collect();
        let pressed = holding.iter().any(|id| !self.asking.contains(id));
        self.asking = holding;
        if pressed && self.can_take() {
            let team = world.resource::<Fights>().team_level;
            let number = self.opened.wrapping_add(1);
            let gangs: Vec<usize> = (0..GANGS.len())
                .filter(|&g| GANGS[g].level <= team)
                .collect();
            let mut rng = Seed(self.seed).child("gang").child(number as u64).rng();
            let gang = gangs[rng.index(gangs.len())];
            self.start_plan(tick, number, gang, None, 0);
        }
    }

    /// A case is closed: karma for the heroes near, and its gang goes.
    fn close(&mut self, world: &mut World, heroes: &[Seen], mut r: Running) {
        reward(world, heroes, r.site().at, REWARD_SHARE);
        r.leave(world);
        self.dry = false;
        self.closed = self.closed.wrapping_add(1);
        let mut news = r.news(world);
        if let ToClient::Case { stage, .. } = &mut news {
            *stage = CLOSED;
        }
        self.last_sent.remove(&r.number);
        self.news.push(news);
    }

    /// A case dropped without being closed (a forced one at its place).
    fn dropped(&mut self, number: u16) {
        if let Some(ToClient::Case { .. }) = self.last_sent.remove(&number) {
            self.news.push(ToClient::Case {
                number,
                stage: IDLE,
                gang: 0,
                step: 0,
                steps: Vec::new(),
                at: [0.0; 3],
                cleared: 0,
                groups: 0,
                boss: 0,
                clues: Vec::new(),
                held: 0,
            });
        }
    }

    fn tell(&mut self, world: &mut World) {
        for r in &self.cases {
            let news = r.news(world);
            if self.last_sent.get(&r.number) != Some(&news) {
                self.last_sent.insert(r.number, news.clone());
                self.news.push(news);
            }
        }
        let desk = ToClient::CaseDesk {
            at: self.contact.unwrap_or(DVec3::ZERO).as_vec3().to_array(),
            closed: self.closed,
            open: (self.cases.len() + self.pending.len()).min(255) as u8,
            limit: self.limit.min(255) as u8,
            can_take: self.can_take(),
        };
        if self.desk_sent.as_ref() != Some(&desk) {
            self.desk_sent = Some(desk.clone());
            self.news.push(desk);
        }
    }

    /// For a player who joins.
    pub fn snapshot(&self) -> Vec<ToClient> {
        self.desk_sent
            .iter()
            .cloned()
            .chain(self.last_sent.values().cloned())
            .collect()
    }

    pub fn take_news(&mut self) -> Vec<ToClient> {
        std::mem::take(&mut self.news)
    }
}

/// Karma for the heroes near `at`: `share` of what their next level takes.
fn reward(world: &mut World, heroes: &[Seen], at: DVec3, share: f64) {
    let rewards: Vec<combat::Reward> = heroes
        .iter()
        .filter(|h| running::near(h.at, at, REWARD))
        .map(|h| combat::Reward {
            hero: h.id,
            karma: (progress::karma_to_next(h.level) as f64 * share).round() as u32,
        })
        .collect();
    world.resource_mut::<Fights>().rewards.extend(rewards);
}

pub(crate) struct Seen {
    pub id: NetId,
    pub at: DVec3,
    pub level: u8,
    /// Holding the help key, on their feet.
    pub help: bool,
}

fn heroes(world: &mut World) -> Vec<Seen> {
    let mut v: Vec<(crate::PlayerId, Seen)> = world
        .query::<(&Hero, &Net, &Motion, &Fighter, &Control)>()
        .iter(world)
        .map(|(h, n, m, f, c)| {
            (
                h.owner,
                Seen {
                    id: n.0,
                    at: m.0.pos,
                    level: f.level,
                    help: c.0.revive && f.standing(),
                },
            )
        })
        .collect();
    v.sort_by_key(|x| x.0);
    v.into_iter().map(|x| x.1).collect()
}
