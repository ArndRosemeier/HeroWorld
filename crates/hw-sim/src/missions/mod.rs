//! Cases: missions made up from the town. The police computer
//! (Crime-o-Vision, in the HUD) lists crimes, gang hideouts to bust
//! ([`board`]); a hero takes one and a case opens on it.
//!
//! A case is a short chain of steps ([`plan`]); a hideout's is one step in
//! one building with a floor plan: arrest the boss ([`CaseKind::Takeover`]
//! in its rooms) or find the thing ([`CaseKind::Evidence`], one real
//! object). The other kinds of step and place ([`sites`]: streets, plazas,
//! the utility tunnels, tube stations, the Deeps, the Doctor's lab) are
//! parked: the code stays, the board does not offer them.
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
//! says how many), never two at one place, each with its own number (the
//! crime's). Done: karma for the heroes near (a share of their next
//! level). The open cases and the counts are kept in the host's hero file
//! ([`hw_net::CaseBook`]) and opened again on the next start. Leave a
//! place for long and the gang goes; come back and it stands again,
//! fresh. Crimes are made up from the world's seed and their number: the
//! same town gives the same crimes.

pub mod board;
pub mod plan;
mod running;
pub mod sites;

pub use plan::{Plan, Step};
pub use running::{safe_front, safe_middle, safe_solid, Running};
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
use std::sync::Arc;

/// The Hero Hall hero who used to hand the cases out (by index): she
/// keeps the hall, and does not answer calls.
pub const CONTACT: usize = 1;
pub const CONTACT_NAME: &str = crate::npcheroes::NAMES[CONTACT];
/// The gang takes its rooms once a hero is this near the building (m).
pub const WAKE: f64 = 90.0;
/// No hero this near for [`AWAY_SECONDS`]: the gang leaves (m).
pub const AWAY: f64 = 250.0;
pub const AWAY_SECONDS: f64 = 30.0;
/// Rooms the gang holds besides the boss's.
pub const ROOMS: usize = 6;
/// Rooms held (besides the boss's) in a small, a middling and a big
/// hideout, and the share of the next level's karma each pays.
pub const SIZE_ROOMS: [usize; 3] = [2, 4, ROOMS];
pub const SIZE_SHARE: [f64; 3] = [0.15, 0.25, 0.4];
/// Each level the gang stands over the team adds this share of the reward.
pub const LEVEL_BONUS: f64 = 0.1;
pub fn rooms_of(size: u8) -> usize {
    SIZE_ROOMS[(size as usize).min(SIZE_ROOMS.len() - 1)]
}
/// The reward for a hideout of `size` whose gang stands `extra` levels
/// over the team: a share of the karma the next level takes.
pub fn share_of(size: u8, extra: u8) -> f64 {
    SIZE_SHARE[(size as usize).min(SIZE_SHARE.len() - 1)] * (1.0 + LEVEL_BONUS * extra as f64)
}
/// Lookouts at a hideout's way in, and the crew that cuts off the way out
/// once the thing is taken.
pub const DOOR_SIZE: u32 = 2;
pub const CUTOFF_SIZE: u32 = 2;
/// A crew that has fought this long and still stands calls the next room.
pub const ALARM_SECONDS: f64 = 6.0;
/// A crew is in a fight while the hero it is after is this near (m).
pub const FIGHT_REACH: f64 = 30.0;
/// The boss breaks off below this share of its health, for this long.
pub const BOSS_BREAKS: f32 = 0.4;
pub const BOSS_RUN_SECONDS: f64 = 9.0;
/// Out of the building by this much (m), the thing is safe.
pub const OUT_MARGIN: f64 = 3.0;
/// Things an evidence case hides: one, a real object.
pub const CLUES: usize = 1;
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
/// A thing still locked in its safe: break the safe open first.
pub const LOCKED: u8 = 3;
/// A gang's safe: half its width and depth (m), its height, and how far
/// in front of its door its body (what heroes aim at) is.
pub const SAFE_HALF: glam::DVec2 = glam::DVec2::new(0.55, 0.42);
pub const SAFE_HEIGHT: f64 = 1.5;
pub const SAFE_FRONT: f64 = 0.14;
/// An opened safe of a closed case is taken away once no hero is within
/// this of it (m).
pub const SAFE_GONE: f64 = 90.0;
/// Bystander numbers of hostages (apart from incidents' and street
/// crimes'): this bit, the case's number and the hostage's.
pub(crate) const HOSTAGE_IDS: u32 = 0x4000_0000;
/// The one who thanks the heroes outside a hideout (in a hostage's place).
pub(crate) const OWNER: u32 = 15;
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

/// The cases: those being worked. Nothing here assumes one case:
/// [`Missions::limit`] says how many may be open at once (never two at
/// one place), and every case keeps its own groups, clues and hostages.
pub struct Missions {
    places: Option<Arc<Places>>,
    seed: u64,
    pub cases: Vec<Running>,
    /// Crimes heroes asked to take, opened on the next tick.
    requests: Vec<u16>,
    /// Cases closed so far (the board starts after them).
    pub closed: u16,
    /// The highest case number handed out so far.
    pub opened: u16,
    /// Most cases open at once.
    pub limit: usize,
    /// The crimes on the board: each until its tick, unless taken.
    slots: Vec<Slot>,
    /// The highest crime number put on the board so far.
    issued: u16,
    /// Crimes that went off the board untaken, and the last one's place.
    missed: u16,
    lost: String,
    /// Streets made rough by a crime nobody took: where, until which tick
    /// (taken by the server for [`crate::incidents`]).
    pub emboldened: Vec<(glam::DVec2, u64)>,
    /// Safes of closed cases, standing until nobody is near.
    left: Vec<running::Safe>,
    /// Doors the police are called to (and the way each looks out), for
    /// the server to take: a hideout case just closed there.
    pub police_calls: Vec<running::PoliceCall>,
    now: u64,
    last_sent: BTreeMap<u16, ToClient>,
    board_sent: Option<ToClient>,
    news: Vec<ToClient>,
    /// The book last written into the heroes' files (or taken from the
    /// host's): a host's book that differs was loaded, and is adopted.
    last_book: CaseBook,
}

impl Default for Missions {
    fn default() -> Self {
        Missions {
            places: None,
            seed: 0,
            cases: Vec::new(),
            requests: Vec::new(),
            closed: 0,
            opened: 0,
            limit: OPEN_LIMIT,
            slots: Vec::new(),
            issued: 0,
            missed: 0,
            lost: String::new(),
            emboldened: Vec::new(),
            left: Vec::new(),
            police_calls: Vec::new(),
            now: 0,
            last_sent: BTreeMap::new(),
            board_sent: None,
            news: Vec::new(),
            last_book: CaseBook::default(),
        }
    }
}

/// The rooms a gang can hold in each building of `floors`.
fn room_counts(floors: &Floors) -> Vec<u8> {
    floors
        .buildings
        .iter()
        .map(|b| running::gang_rooms(b).len().min(255) as u8)
        .collect()
}

/// A crime on the board.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Slot {
    number: u16,
    /// It goes off the board at this tick if nobody took it.
    until: u64,
}

/// Hero Hall's contact hero (the cases keep clear of her).
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
    /// the buildings with a route map (the hideouts among them).
    pub fn plan(world: &Arc<TownWorld>, area: Arc<Area>, floors: Arc<Floors>, seed: u64) -> Self {
        let contact = hall_contact(world);
        let mut names: Vec<String> = world
            .landmarks
            .iter()
            .filter(|l| hw_gen::urban::comet::layout(l).is_some())
            .map(|l| hw_gen::urban::comet::name(l).to_string())
            .collect();
        // Hideouts: "the tenement on Juniper Street", in a seeded order.
        names.resize(floors.buildings.len(), String::new());
        for &(n, k) in &floors.hideouts {
            let site = &area.buildings[n].site;
            names[k] = format!(
                "{} on {}",
                sites::hideout_noun(site),
                sites::street_near(&area, site.origin)
            );
        }
        let mut hideouts: Vec<usize> = floors.hideouts.iter().map(|h| h.1).collect();
        hideouts.sort_by_key(|&k| Seed(seed).child("hideout").child(k as u64).rng().next_u64());
        let keep_clear = contact
            .iter()
            .map(|c| glam::DVec2::new(c.x, c.z))
            .chain(std::iter::once(world.spawn_line().0))
            .collect();
        Missions {
            places: Some(Arc::new(Places {
                town: Some(world.clone()),
                area: Some(area),
                rooms: room_counts(&floors),
                floors,
                names,
                hideouts,
                keep_clear,
            })),
            seed,
            ..Default::default()
        }
    }

    /// Cases in the buildings of `floors` only, every one a hideout (tests).
    pub fn new(floors: Arc<Floors>, seed: u64) -> Self {
        let n = floors.buildings.len();
        Missions {
            places: Some(Arc::new(Places {
                rooms: room_counts(&floors),
                floors,
                names: vec!["test hall".to_string(); n],
                hideouts: (0..n).collect(),
                ..Default::default()
            })),
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

    /// Opens crime `number` of the board now, past the board's window and
    /// the limit (tests and screenshots). Its number.
    pub fn force_crime(&mut self, world: &mut World, number: u16) -> Option<u16> {
        let places = self.places.clone()?;
        let c = board::crime(&places, self.seed, number)?;
        let team = world.resource::<Fights>().team_level;
        let plan = plan::crime_plan(
            &places,
            self.seed,
            number,
            board::gang_for(self.seed, number, team),
        )?;
        while let Some(k) = self
            .cases
            .iter()
            .position(|r| r.number == number || r.site().building == Some(c.building))
        {
            let mut r = self.cases.remove(k);
            r.leave(world);
            self.dropped(r.number);
        }
        self.opened = self.opened.max(number);
        self.insert(Running::new(number, plan, 0, places.floors.clone()));
        Some(number)
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
            crime: Default::default(),
            extra: 0,
            size: 2,
            hot: false,
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

    /// Does this town have places to hold cases?
    pub fn available(&self) -> bool {
        self.places
            .as_ref()
            .is_some_and(|p| !p.floors.buildings.is_empty() || p.town.is_some())
    }

    /// A hero asks to take crime `number` from the board: it opens on the
    /// next tick if the crime is on the board, free, and a case may open.
    pub fn take_case(&mut self, number: u16) {
        if !self.requests.contains(&number) {
            self.requests.push(number);
        }
    }

    /// The places of the open cases (what a new one keeps clear of).
    fn taken(&self) -> Vec<Site> {
        self.cases
            .iter()
            .flat_map(|r| r.steps()[r.step..].iter().map(|s| s.site.clone()))
            .collect()
    }

    /// Opens the crimes heroes asked for.
    fn open_requested(&mut self, world: &mut World) {
        let Some(places) = self.places.clone() else {
            return;
        };
        for number in std::mem::take(&mut self.requests) {
            let on_board = self.slots.iter().any(|s| s.number == number);
            if !on_board || self.cases.len() >= self.limit {
                continue;
            }
            let Some(c) = board::crime(&places, self.seed, number) else {
                continue;
            };
            if self
                .cases
                .iter()
                .any(|r| r.number == number || r.site().building == Some(c.building))
            {
                continue;
            }
            let team = world.resource::<Fights>().team_level;
            let gang = board::gang_for(self.seed, number, team);
            let Some(plan) = plan::crime_plan(&places, self.seed, number, gang) else {
                continue;
            };
            self.opened = self.opened.max(number);
            self.insert(Running::new(number, plan, 0, places.floors.clone()));
        }
    }

    /// Once a second: crimes nobody took go off the board when their time
    /// is up (the streets round their hideout are rough for a while), and
    /// new ones come up.
    fn turn_board(&mut self, tick: u64) {
        let Some(places) = self.places.clone() else {
            return;
        };
        let ticks = |s: f64| (s * crate::TICK_HZ as f64) as u64;
        let open: Vec<u16> = self.cases.iter().map(|r| r.number).collect();
        let (stay, gone): (Vec<Slot>, Vec<Slot>) = std::mem::take(&mut self.slots)
            .into_iter()
            .partition(|s| tick < s.until || open.contains(&s.number));
        self.slots = stay;
        for s in gone {
            let Some(c) = board::crime(&places, self.seed, s.number) else {
                continue;
            };
            let site = sites::building_site(&places, c.building);
            self.missed = self.missed.wrapping_add(1);
            self.lost = places.names.get(c.building).cloned().unwrap_or_default();
            self.emboldened.push((
                glam::DVec2::new(site.at.x, site.at.z),
                tick + ticks(board::EMBOLDENED_SECONDS),
            ));
        }
        self.emboldened.retain(|e| tick < e.1);
        while self.slots.len() < board::BOARD && self.issued < u16::MAX {
            let number = self.issued + 1;
            let Some(c) = board::crime(&places, self.seed, number) else {
                break;
            };
            self.issued = number;
            // A crime already open (from the host's file) is not offered.
            if open.contains(&number) {
                continue;
            }
            self.slots.push(Slot {
                number,
                until: tick + ticks(board::life(self.seed, number, c.hot)),
            });
        }
    }

    /// A case in the list, in the order of its number.
    fn insert(&mut self, r: Running) {
        let at = self
            .cases
            .iter()
            .position(|c| c.number > r.number)
            .unwrap_or(self.cases.len());
        self.cases.insert(at, r);
    }

    /// The crimes on the board now.
    fn board(&self, world: &mut World) -> ToClient {
        let Some(places) = &self.places else {
            return ToClient::CaseBoard {
                crimes: Vec::new(),
                limit: self.limit.min(255) as u8,
                open: 0,
                missed: 0,
                lost: String::new(),
            };
        };
        let team = world.resource::<Fights>().team_level;
        let crimes = self
            .slots
            .iter()
            .filter_map(|slot| {
                let number = slot.number;
                let c = board::crime(places, self.seed, number)?;
                let case = self.cases.iter().find(|r| r.number == number);
                let gang =
                    case.map_or_else(|| board::gang_for(self.seed, number, team), |r| r.gang);
                let site = sites::building_site(places, c.building);
                let name = places.names.get(c.building).cloned().unwrap_or_default();
                let mut chars = name.chars();
                let name = chars
                    .next()
                    .map(|f| f.to_uppercase().chain(chars).collect::<String>())
                    .unwrap_or_default();
                Some(hw_net::BoardCrime {
                    number,
                    crime: c.kind.code(),
                    job: c.job.code(),
                    gang: gang as u8,
                    level: team.saturating_add(c.extra),
                    size: c.size,
                    hot: c.hot,
                    // Whole minutes left, rounded up (a taken one stays).
                    minutes: if case.is_some() {
                        255
                    } else {
                        let left = slot.until.saturating_sub(self.now);
                        left.div_ceil(60 * crate::TICK_HZ as u64).min(254) as u8
                    },
                    at: site.at.as_vec3().to_array(),
                    name,
                    taken: case.is_some()
                        || self
                            .cases
                            .iter()
                            .any(|r| r.site().building == Some(c.building)),
                })
            })
            .collect();
        ToClient::CaseBoard {
            crimes,
            limit: self.limit.min(255) as u8,
            open: self.cases.len().min(255) as u8,
            missed: self.missed,
            lost: self.lost.clone(),
        }
    }

    /// Every tick: searching at clues and hostages, crimes taken. Once a second: gangs standing at their places,
    /// steps and cases done.
    pub(crate) fn step(&mut self, world: &mut World, tick: u64) {
        crate::incidents::walk_ways(world, tick);
        if !self.available() {
            return;
        }
        self.now = tick;
        if tick.is_multiple_of(crate::TICK_HZ as u64) || self.slots.is_empty() {
            self.turn_board(tick);
        }
        self.open_requested(world);
        let heroes = heroes(world);
        if heroes.is_empty() {
            return;
        }
        for r in &mut self.cases {
            r.search(world, tick, &heroes);
        }
        if tick.is_multiple_of(crate::TICK_HZ as u64) {
            // Safes of closed cases go once nobody is there to see it.
            let (stay, go): (Vec<_>, Vec<_>) = std::mem::take(&mut self.left)
                .into_iter()
                .partition(|s| heroes.iter().any(|h| h.at.distance(s.at) < SAFE_GONE));
            self.left = stay;
            for s in go {
                s.remove(world);
            }
            self.keep_book(world);
            let mut done = Vec::new();
            for (k, r) in self.cases.iter_mut().enumerate() {
                match r.settle(world, tick, &heroes) {
                    running::Settled::Working => {}
                    running::Settled::Lead(at) => {
                        reward(world, &heroes, at, LEAD_SHARE, false);
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

    /// Every safe standing in a hideout, as solids: heroes and villains
    /// walk round them (what is drawn is what collides).
    pub fn safe_solids(&self) -> Vec<crate::collision::Solid> {
        self.cases
            .iter()
            .flat_map(|r| r.safes())
            .chain(self.left.iter())
            .map(|s| s.solid())
            .collect()
    }

    /// The cases in the host's file: a book that is new to us (the host
    /// came, a file was loaded) is taken over, open cases and all; then
    /// what we have now is written into every hero's file.
    fn keep_book(&mut self, world: &mut World) {
        let host = world
            .query::<(&Hero, &crate::Progress)>()
            .iter(world)
            .min_by_key(|(h, _)| h.owner)
            .map(|(_, p)| p.sheet.cases.clone());
        if let Some(book) = host.filter(|b| *b != self.last_book) {
            self.adopt(world, &book);
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
    fn adopt(&mut self, world: &mut World, book: &CaseBook) {
        for mut r in std::mem::take(&mut self.cases) {
            r.leave(world);
            self.dropped(r.number);
        }
        self.opened = book.opened;
        self.closed = book.closed;
        // The board starts afresh after what was handed out.
        self.slots.clear();
        self.issued = book.opened;
        if book.world == self.seed {
            if let Some(places) = self.places.clone() {
                for c in &book.open {
                    if (c.gang as usize) >= GANGS.len() || c.variant == FORCED {
                        continue;
                    }
                    if let Some(plan) =
                        plan::crime_plan(&places, self.seed, c.number, c.gang as usize)
                    {
                        let step = (c.step as usize).min(plan.steps.len() - 1);
                        self.insert(Running::new(c.number, plan, step, places.floors.clone()));
                        self.opened = self.opened.max(c.number);
                    }
                }
            }
        }
        self.last_book = book.clone();
    }

    /// A case is closed: karma for the heroes near, and its gang goes.
    fn close(&mut self, world: &mut World, heroes: &[Seen], mut r: Running) {
        reward(
            world,
            heroes,
            r.site().at,
            r.share(),
            r.site().building.is_some(),
        );
        // What the gang had: an augment for each hero there, never a
        // common one.
        let (at, tall) = (r.site().at, r.site().building.is_some());
        let finds: Vec<combat::Drop> = heroes
            .iter()
            .filter(|h| {
                if tall {
                    running::flat(h.at, at) < REWARD
                } else {
                    running::near(h.at, at, REWARD)
                }
            })
            .map(|h| combat::Drop {
                hero: h.id,
                from: h.id,
                at: h.at + DVec3::Y * 1.2,
                carried: None,
                level: h.level,
            })
            .collect();
        world.resource_mut::<Fights>().drops.extend(finds);
        if r.is_hideout() {
            let (safes, police) = r.wrap_up(world, self.now);
            self.left.extend(safes);
            self.police_calls.extend(police);
        } else {
            r.leave(world);
        }
        self.slots.retain(|s| s.number != r.number);
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
                crime: 0,
                level: 0,
                size: 0,
                alarms: 0,
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
        let board = self.board(world);
        if self.board_sent.as_ref() != Some(&board) {
            self.board_sent = Some(board.clone());
            self.news.push(board);
        }
    }

    /// For a player who joins.
    pub fn snapshot(&self) -> Vec<ToClient> {
        self.board_sent
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
/// In a building (`tall`) near means on the flat: a hero on its top floor
/// is as near as one at its door.
fn reward(world: &mut World, heroes: &[Seen], at: DVec3, share: f64, tall: bool) {
    let rewards: Vec<combat::Reward> = heroes
        .iter()
        .filter(|h| {
            if tall {
                running::flat(h.at, at) < REWARD
            } else {
                running::near(h.at, at, REWARD)
            }
        })
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
