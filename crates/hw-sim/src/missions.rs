//! Indoor missions: cases in real buildings. Atomic Annie at Hero Hall
//! hands them out: a hero holds the help key (E) beside her and gets one.
//! The wrist radio leads the way to the building; once a hero comes near,
//! a gang holds its rooms, one group to a room, floor after floor, with
//! its boss in the biggest room at the top.
//!
//! A group notices a hero only as it always does: in sight and in reach.
//! Walls hide the rooms from each other, so a careful hero clears them
//! one at a time (Arnd, 2026-11-11: "not overly aware"). Villains who did
//! notice follow through doorways and up the stairs by the building's
//! route map ([`crate::floornav`]).
//!
//! Three kinds of case:
//! - **Takeover**: beat the boss.
//! - **Evidence**: find three clues spread over the floors (hold E at each).
//! - **Hostages**: three townsfolk held in rooms on different floors; beat
//!   the room's crew, then hold E by them to untie them. They thank their
//!   hero and walk out by the route map.
//!
//! Several cases can be open at once ([`OPEN_LIMIT`], the only place that
//! says how many), one to a building, each with its own number; Annie
//! gives one per press of E. Done: karma for the heroes near (a share of
//! their next level). The open cases and the counts are kept in the host's
//! hero file ([`hw_net::CaseBook`]) and opened again on the next start.
//! Leave the building for long and the gang
//! goes; come back and it stands again, fresh. Cases are made up from the
//! world's seed and a running count: the same town gives the same cases.
//! They play in the buildings that have a floor plan (the Comet buildings
//! today); every building converted to one can hold them.

use crate::combat::{self, room_to_stand, EnemyKind, Fighter, Fights, Spot};
use crate::floornav::Floors;
use crate::{progress, Control, Hero, Motion, Net, DT, TICK_HZ};
use bevy_ecs::prelude::*;
use glam::{DVec2, DVec3};
use hw_core::{NetId, Seed};
use hw_gen::layout::SpaceKind;
use hw_gen::urban::landmarks::LandmarkKind;
use hw_gen::urban::world::TownWorld;
use hw_net::protocol::MAX_SAVED_CASES;
use hw_net::{CaseBook, SavedCase, ToClient};
use std::collections::BTreeMap;
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
const HOSTAGE_IDS: u32 = 0x4000_0000;
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
/// Cases open at once, at most (and never two in one building).
pub const OPEN_LIMIT: usize = 3;

/// What a case asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseKind {
    Takeover,
    Evidence,
    Hostages,
}

impl CaseKind {
    pub fn code(self) -> u8 {
        match self {
            CaseKind::Takeover => 0,
            CaseKind::Evidence => 1,
            CaseKind::Hostages => 2,
        }
    }

    pub fn from_code(c: u8) -> Self {
        match c {
            1 => CaseKind::Evidence,
            2 => CaseKind::Hostages,
            _ => CaseKind::Takeover,
        }
    }
}

/// A gang that takes over buildings: its people, its lieutenant and the
/// team level it comes out at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gang {
    pub name: &'static str,
    /// Who fights up close and who shoots (None: the Doctor's own mix).
    pub crew: Option<(EnemyKind, EnemyKind)>,
    pub boss: EnemyKind,
    pub level: u8,
}

pub const GANGS: [Gang; 5] = [
    Gang {
        name: "the Doctor's technicians",
        crew: None,
        boss: EnemyKind::Sentry,
        level: 1,
    },
    Gang {
        name: "the Chrome Jackets and the Velvet Syndicate",
        crew: Some((EnemyKind::Greaser, EnemyKind::Gunsel)),
        boss: EnemyKind::Alpha,
        level: 1,
    },
    Gang {
        name: "the sky pirates",
        crew: Some((EnemyKind::Buccaneer, EnemyKind::Gunner)),
        boss: EnemyKind::Captain,
        level: 2,
    },
    Gang {
        name: "the Iron-Masked Empire",
        crew: Some((EnemyKind::Legionary, EnemyKind::Legionary)),
        boss: EnemyKind::Praetorian,
        level: crate::incidents::IRON_LEVEL,
    },
    Gang {
        name: "the invaders from Zorrak",
        crew: Some((EnemyKind::Grabber, EnemyKind::Zapper)),
        boss: EnemyKind::Overlord,
        level: crate::incidents::SAUCER_LEVEL,
    },
];

/// One case.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Case {
    pub kind: CaseKind,
    /// The building, by index in [`Floors::buildings`].
    pub building: usize,
    /// The gang, by index in [`GANGS`].
    pub gang: usize,
}

/// A room the gang holds: its fight spot.
#[derive(Clone, Debug)]
struct Room {
    spot: usize,
    at: DVec3,
}

/// A clue to find, or a hostage to free.
#[derive(Clone, Copy, Debug)]
struct Clue {
    at: DVec3,
    found: bool,
    /// A hostage: the room whose crew guards them (in `rooms`), and their
    /// bystander number.
    hostage: Option<(usize, u32)>,
}

/// A case being worked: which, how far along, and the gang's rooms.
#[derive(Clone, Debug)]
pub struct Running {
    /// Its number in the team's cases (1, 2, ...): its name everywhere.
    pub number: u16,
    pub case: Case,
    /// [`GO`] or [`INSIDE`].
    pub stage: u8,
    floors: Arc<Floors>,
    rooms: Vec<Room>,
    boss: Option<Room>,
    clues: Vec<Clue>,
    /// Seconds of holding the help key at a clue or hostage.
    held: f64,
    /// Since when no hero has been near the building (a tick).
    away_since: Option<u64>,
}

/// The cases: those being worked, and Annie's desk. Nothing here assumes
/// one case: [`Missions::limit`] says how many may be open at once (one
/// to a building at most), and every case keeps its own rooms, clues and
/// hostages.
pub struct Missions {
    floors: Option<Arc<Floors>>,
    /// Where Annie stands.
    contact: Option<DVec3>,
    seed: u64,
    pub cases: Vec<Running>,
    /// Cases closed so far.
    pub closed: u16,
    /// Cases handed out so far (the next one's number is one more).
    pub opened: u16,
    /// Most cases open at once.
    pub limit: usize,
    /// Heroes holding the help key by Annie (a new case takes a new press).
    asking: Vec<NetId>,
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
            floors: None,
            contact: None,
            seed: 0,
            cases: Vec::new(),
            closed: 0,
            opened: 0,
            limit: OPEN_LIMIT,
            asking: Vec::new(),
            last_sent: BTreeMap::new(),
            desk_sent: None,
            news: Vec::new(),
            last_book: CaseBook::default(),
        }
    }
}

/// The middle of a building's way in, for the radio.
fn entrance(floors: &Floors, k: usize) -> DVec3 {
    let b = &floors.buildings[k];
    b.to_town(b.graph.front())
}

impl Missions {
    /// Cases in `world` (the town), in the buildings with a route map.
    pub fn plan(world: &TownWorld, floors: Arc<Floors>, seed: u64) -> Self {
        let contact = world
            .landmarks
            .iter()
            .find(|l| l.kind == LandmarkKind::HeroHall)
            .and_then(|hall| {
                hall.hero_spots()
                    .get(CONTACT)
                    .map(|(p, _)| DVec3::new(p.x, hall.base, p.y))
            });
        Missions {
            floors: Some(floors),
            contact,
            seed,
            ..Default::default()
        }
    }

    /// Cases in the buildings of `floors`, handed out at `contact` (tests).
    pub fn new(floors: Arc<Floors>, contact: Option<DVec3>, seed: u64) -> Self {
        Missions {
            floors: Some(floors),
            contact,
            seed,
            ..Default::default()
        }
    }

    /// Opens `case` now (tests and screenshots), dropping a case in the
    /// same building. Its number.
    pub fn force(&mut self, world: &mut World, case: Case) -> u16 {
        if let Some(k) = self
            .cases
            .iter()
            .position(|r| r.case.building == case.building)
        {
            let mut r = self.cases.remove(k);
            r.leave(world);
            self.dropped(r.number);
        }
        self.open(case)
    }

    fn open(&mut self, case: Case) -> u16 {
        self.opened = self.opened.wrapping_add(1);
        let number = self.opened;
        self.cases.push(Running {
            number,
            case,
            stage: GO,
            floors: self.floors.clone().expect("available"),
            rooms: Vec::new(),
            boss: None,
            clues: Vec::new(),
            held: 0.0,
            away_since: None,
        });
        number
    }

    /// The case with this number, if open.
    pub fn case(&self, number: u16) -> Option<&Running> {
        self.cases.iter().find(|r| r.number == number)
    }

    /// Does this town have cases (a contact and a building to hold them)?
    /// How many buildings can hold a case.
    pub fn buildings(&self) -> usize {
        self.floors.as_ref().map_or(0, |f| f.buildings.len())
    }

    pub fn available(&self) -> bool {
        self.contact.is_some()
            && self
                .floors
                .as_ref()
                .is_some_and(|f| !f.buildings.is_empty())
    }

    /// Buildings with no open case.
    fn free_buildings(&self) -> Vec<usize> {
        let n = self.floors.as_ref().map_or(0, |f| f.buildings.len());
        (0..n)
            .filter(|k| self.cases.iter().all(|r| r.case.building != *k))
            .collect()
    }

    /// Can Annie hand out another case now?
    pub fn can_take(&self) -> bool {
        self.available() && self.cases.len() < self.limit && !self.free_buildings().is_empty()
    }

    /// The next case: kind, gang and a building with no case in it.
    fn make_case(&self, team: u8) -> Option<Case> {
        let free = self.free_buildings();
        if free.is_empty() {
            return None;
        }
        let mut rng = Seed(self.seed)
            .child("case")
            .child(self.opened as u64)
            .rng();
        let gangs: Vec<usize> = (0..GANGS.len())
            .filter(|&g| GANGS[g].level <= team)
            .collect();
        let kind = [CaseKind::Takeover, CaseKind::Evidence, CaseKind::Hostages][rng.index(3)];
        Some(Case {
            kind,
            building: free[rng.index(free.len())],
            gang: gangs[rng.index(gangs.len())],
        })
    }

    /// Every tick: searching at clues and hostages, asking Annie. Once a
    /// second: gangs taking their rooms, cases closed.
    pub(crate) fn step(&mut self, world: &mut World, tick: u64) {
        crate::incidents::walk_ways(world, tick);
        if !self.available() {
            return;
        }
        let heroes = heroes(world);
        if heroes.is_empty() {
            return;
        }
        for r in &mut self.cases {
            if r.stage == INSIDE {
                r.search(world, tick, &heroes);
            }
        }
        self.ask(world, &heroes);
        if tick.is_multiple_of(TICK_HZ as u64) {
            self.keep_book(world);
            let mut done = Vec::new();
            for (k, r) in self.cases.iter_mut().enumerate() {
                if r.settle(world, tick, &heroes) {
                    done.push(k);
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

    /// The cases as a hero file keeps them.
    pub fn book(&self) -> CaseBook {
        CaseBook {
            world: self.seed,
            opened: self.opened,
            closed: self.closed,
            open: self
                .cases
                .iter()
                .take(MAX_SAVED_CASES)
                .map(|r| SavedCase {
                    number: r.number,
                    kind: r.case.kind.code(),
                    gang: r.case.gang.min(255) as u8,
                    building: r.case.building.min(255) as u8,
                })
                .collect(),
        }
    }

    /// Takes over a saved book: its counts, and its open cases if they are
    /// of this world (a building each, gangs that exist).
    fn adopt(&mut self, world: &mut World, book: &CaseBook) {
        for mut r in std::mem::take(&mut self.cases) {
            r.leave(world);
            self.dropped(r.number);
        }
        self.opened = book.opened;
        self.closed = book.closed;
        let buildings = self.floors.as_ref().map_or(0, |f| f.buildings.len());
        if book.world == self.seed {
            for c in &book.open {
                let building = c.building as usize;
                let taken = self.cases.iter().any(|r| r.case.building == building);
                if building >= buildings || taken || c.gang as usize >= GANGS.len() {
                    continue;
                }
                self.cases.push(Running {
                    number: c.number,
                    case: Case {
                        kind: CaseKind::from_code(c.kind),
                        building,
                        gang: c.gang as usize,
                    },
                    stage: GO,
                    floors: self.floors.clone().expect("available"),
                    rooms: Vec::new(),
                    boss: None,
                    clues: Vec::new(),
                    held: 0.0,
                    away_since: None,
                });
                self.opened = self.opened.max(c.number);
            }
        }
        self.last_book = book.clone();
    }

    /// A hero pressing the help key by Annie gets a case, if she has one.
    fn ask(&mut self, world: &mut World, heroes: &[Seen]) {
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
            if let Some(c) = self.make_case(team) {
                self.open(c);
            }
        }
    }

    /// A case is closed: karma for the heroes near, and its gang goes.
    fn close(&mut self, world: &mut World, heroes: &[Seen], mut r: Running) {
        let at = entrance(&r.floors, r.case.building);
        let rewards: Vec<combat::Reward> = heroes
            .iter()
            .filter(|h| flat(h.at, at) < REWARD)
            .map(|h| combat::Reward {
                hero: h.id,
                karma: (progress::karma_to_next(h.level) as f64 * REWARD_SHARE).round() as u32,
            })
            .collect();
        world.resource_mut::<Fights>().rewards.extend(rewards);
        r.leave(world);
        self.closed = self.closed.wrapping_add(1);
        let mut news = r.news(world);
        if let ToClient::Case { stage, .. } = &mut news {
            *stage = CLOSED;
        }
        self.last_sent.remove(&r.number);
        self.news.push(news);
    }

    /// A case dropped without being closed (a forced one in its building).
    fn dropped(&mut self, number: u16) {
        if let Some(ToClient::Case { .. }) = self.last_sent.remove(&number) {
            self.news.push(ToClient::Case {
                number,
                stage: IDLE,
                kind: 0,
                gang: 0,
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
            open: self.cases.len().min(255) as u8,
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

impl Running {
    /// Where the clues (or hostages) are and whether each is found (freed).
    pub fn clues(&self) -> Vec<(DVec3, bool)> {
        self.clues.iter().map(|k| (k.at, k.found)).collect()
    }

    /// The hostages' bystander numbers.
    pub fn hostages(&self) -> Vec<u32> {
        self.clues
            .iter()
            .filter_map(|k| k.hostage.map(|h| h.1))
            .collect()
    }

    /// Where to go: the building's way in, or inside the next clue or
    /// hostage, or the boss.
    pub fn objective(&self) -> DVec3 {
        let way_in = entrance(&self.floors, self.case.building);
        match (self.stage, self.case.kind) {
            (INSIDE, CaseKind::Evidence | CaseKind::Hostages) => self
                .clues
                .iter()
                .find(|k| !k.found)
                .map_or(way_in, |k| k.at),
            (INSIDE, CaseKind::Takeover) => self.boss.as_ref().map_or(way_in, |b| b.at),
            _ => way_in,
        }
    }

    /// Once a second: the gang takes its rooms when a hero comes near,
    /// leaves when they are long gone. True once the case is done.
    fn settle(&mut self, world: &mut World, tick: u64, heroes: &[Seen]) -> bool {
        let at = entrance(&self.floors, self.case.building);
        match self.stage {
            GO => {
                if heroes.iter().any(|h| flat(h.at, at) < WAKE) {
                    self.take_rooms(world, tick, heroes.len());
                    self.stage = INSIDE;
                    self.away_since = None;
                }
                false
            }
            INSIDE => {
                if heroes.iter().all(|h| flat(h.at, at) > AWAY) {
                    let since = *self.away_since.get_or_insert(tick);
                    if tick >= since + (AWAY_SECONDS * TICK_HZ as f64) as u64 {
                        // Gone too long: the gang slips away, and stands
                        // there fresh when a hero comes back.
                        self.leave(world);
                        self.stage = GO;
                    }
                    return false;
                }
                self.away_since = None;
                match self.case.kind {
                    CaseKind::Takeover => self.boss.as_ref().is_some_and(|b| beaten(world, b.spot)),
                    CaseKind::Evidence | CaseKind::Hostages => self.clues.iter().all(|k| k.found),
                }
            }
            _ => false,
        }
    }

    /// What the clients are told of it.
    fn news(&self, world: &mut World) -> ToClient {
        let (cleared, groups) = self.cleared(world);
        let boss = match &self.boss {
            None => 0,
            Some(b) if beaten(world, b.spot) => 2,
            Some(_) => 1,
        };
        let held = ((self.held / self.hold_seconds()).clamp(0.0, 1.0) * 255.0) as u8 & !15;
        ToClient::Case {
            number: self.number,
            stage: self.stage,
            kind: self.case.kind.code(),
            gang: self.case.gang as u8,
            at: self.objective().as_vec3().to_array(),
            cleared,
            groups,
            boss,
            clues: (0..self.clues.len())
                .map(|k| (self.clues[k].at.as_vec3().to_array(), self.state(world, k)))
                .collect(),
            held,
        }
    }

    /// The gang takes its rooms: a group in each of [`ROOMS`] rooms over
    /// the floors, the boss and its guard in the biggest room on top, and
    /// the clues of an evidence case.
    fn take_rooms(&mut self, world: &mut World, tick: u64, heroes: usize) {
        let floors = self.floors.clone();
        let c = self.case;
        let b = &floors.buildings[c.building];
        let gang = GANGS[c.gang];
        let solids = world.resource::<crate::Solids>().0.clone();
        let mut rooms: Vec<(SpaceKind, hw_gen::geom::Rect, f64)> = b
            .graph
            .rooms()
            .into_iter()
            .filter(|(k, r, _)| {
                matches!(
                    k,
                    SpaceKind::Hall
                        | SpaceKind::Boardroom
                        | SpaceKind::Lobby
                        | SpaceKind::Shop
                        | SpaceKind::Concourse
                        | SpaceKind::Vestibule
                ) && r.size().min_element() >= 8.0
            })
            .collect();
        // Lowest first, then the biggest.
        rooms.sort_by(|a, b| {
            a.2.total_cmp(&b.2)
                .then(b.1.area().total_cmp(&a.1.area()))
                .then(a.1.min.x.total_cmp(&b.1.min.x))
                .then(a.1.min.y.total_cmp(&b.1.min.y))
        });
        let Some(top) = rooms
            .iter()
            .filter(|r| (r.2 - rooms.last().map_or(0.0, |t| t.2)).abs() < 1e-3)
            .max_by(|a, b| {
                let boardroom = |r: &(SpaceKind, _, _)| r.0 == SpaceKind::Boardroom;
                boardroom(a)
                    .cmp(&boardroom(b))
                    .then(a.1.area().total_cmp(&b.1.area()))
            })
            .copied()
        else {
            return;
        };
        let rest: Vec<_> = rooms.iter().filter(|r| **r != top).copied().collect();
        // Spread over the floors: one room per floor, round and round.
        let mut levels: Vec<f64> = rest.iter().map(|r| r.2).collect();
        levels.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
        let mut chosen = Vec::new();
        let mut round = 0;
        while chosen.len() < ROOMS && chosen.len() < rest.len() {
            for &y in &levels {
                let on: Vec<_> = rest.iter().filter(|r| (r.2 - y).abs() < 1e-3).collect();
                if let Some(r) = on.get(round) {
                    if chosen.len() < ROOMS {
                        chosen.push(**r);
                    }
                }
            }
            round += 1;
            if round > 8 {
                break;
            }
        }
        let extra = heroes.saturating_sub(1) as u32;
        let place =
            |world: &mut World, r: &(SpaceKind, hw_gen::geom::Rect, f64), size, lead, boss| {
                let at = open_middle(&solids, b, &r.1, r.2)?;
                // Facing the way in: heroes come from the street.
                let door = b.to_town(b.graph.front());
                let facing = DVec2::new(door.x - at.x, door.z - at.z).normalize_or(DVec2::X);
                let spot = Spot {
                    extra: if boss { BOSS_EXTRA } else { 0 },
                    size: Some(size),
                    crew: gang.crew,
                    once: true,
                    lead,
                    ..Spot::fight(at, facing)
                };
                let index = {
                    let mut f = world.resource_mut::<Fights>();
                    f.spots.push(spot);
                    f.spots.len() - 1
                };
                combat::spawn_group(world, tick, index, heroes);
                Some(Room { spot: index, at })
            };
        let placed: Vec<(Room, (SpaceKind, hw_gen::geom::Rect, f64))> = chosen
            .iter()
            .filter_map(|r| place(world, r, ROOM_SIZE + extra, None, false).map(|p| (p, *r)))
            .collect();
        let chosen: Vec<_> = placed.iter().map(|p| p.1).collect();
        self.rooms = placed.into_iter().map(|p| p.0).collect();
        self.boss = place(world, &top, BOSS_SIZE + extra, Some(gang.boss), true);
        self.clues.clear();
        if c.kind != CaseKind::Takeover {
            let want = if c.kind == CaseKind::Hostages {
                HOSTAGES
            } else {
                CLUES
            };
            // In rooms on different floors (high ones first), tucked away
            // from where each room's group stands; a second round fills
            // up when there are fewer floors than clues.
            let mut order: Vec<usize> = (0..self.rooms.len()).collect();
            order.sort_by(|&a, &b| chosen[b].2.total_cmp(&chosen[a].2).then(a.cmp(&b)));
            let mut used: Vec<usize> = Vec::new();
            for second in [false, true] {
                for &i in &order {
                    if self.clues.len() >= want || used.contains(&i) {
                        continue;
                    }
                    let y = chosen[i].2;
                    let floor_taken = used.iter().any(|&u| (chosen[u].2 - y).abs() < 1e-3);
                    if floor_taken && !second {
                        continue;
                    }
                    if let Some(at) = far_corner(&solids, b, &chosen[i].1, y, self.rooms[i].at) {
                        let hostage = (c.kind == CaseKind::Hostages).then(|| {
                            let id = HOSTAGE_IDS
                                | ((self.number as u32 & 0xffff) << 4)
                                | self.clues.len() as u32;
                            // Hands up, facing their guards.
                            let g = self.rooms[i].at;
                            let face = DVec2::new(g.x - at.x, g.z - at.z).normalize_or(DVec2::X);
                            crate::incidents::spawn_bystander(world, tick, id, at, face, None);
                            (i, id)
                        });
                        self.clues.push(Clue {
                            at,
                            found: false,
                            hostage,
                        });
                        used.push(i);
                    }
                }
            }
        }
        self.held = 0.0;
    }

    /// The gang leaves (its spots stay, empty, never refilled).
    fn leave(&mut self, world: &mut World) {
        let spots: Vec<usize> = self
            .rooms
            .iter()
            .chain(self.boss.iter())
            .map(|r| r.spot)
            .collect();
        let mut gone: Vec<(NetId, Entity)> = world
            .query::<(Entity, &Net, &combat::Enemy)>()
            .iter(world)
            .filter(|(_, _, en)| spots.contains(&en.spot))
            .map(|(e, n, _)| (n.0, e))
            .collect();
        let held: Vec<u32> = self
            .clues
            .iter()
            .filter(|k| !k.found)
            .filter_map(|k| k.hostage.map(|h| h.1))
            .collect();
        gone.extend(
            world
                .query::<(Entity, &Net, &crate::incidents::Bystander)>()
                .iter(world)
                .filter(|(_, _, b)| held.contains(&b.incident))
                .map(|(e, n, _)| (n.0, e)),
        );
        gone.sort_by_key(|g| g.0);
        for (id, e) in gone {
            world.resource_mut::<Fights>().paths.remove(&id);
            world.despawn(e);
        }
        {
            let mut f = world.resource_mut::<Fights>();
            for s in spots {
                f.spots[s].members = 0;
                f.spots[s].respawn_at = None;
            }
        }
        self.rooms.clear();
        self.boss = None;
        self.clues.clear();
        self.held = 0.0;
    }

    /// Heroes holding the help key at a clue, or by a hostage whose
    /// guards are beaten.
    fn search(&mut self, world: &mut World, tick: u64, heroes: &[Seen]) {
        let states: Vec<u8> = (0..self.clues.len())
            .map(|k| self.state(world, k))
            .collect();
        let Some(k) = (0..self.clues.len()).find(|&k| {
            states[k] == READY
                && heroes
                    .iter()
                    .any(|h| h.help && h.at.distance(self.clues[k].at) < CLUE_REACH)
        }) else {
            self.held = (self.held - DT).max(0.0);
            return;
        };
        self.held += DT;
        if self.held + 1e-9 >= self.hold_seconds() {
            self.clues[k].found = true;
            self.held = 0.0;
            if let Some((_, id)) = self.clues[k].hostage {
                self.set_free(world, tick, id);
            }
        }
    }

    /// Seconds of holding E a clue or a hostage takes.
    fn hold_seconds(&self) -> f64 {
        match self.case.kind {
            CaseKind::Hostages => FREE_SECONDS,
            _ => CLUE_SECONDS,
        }
    }

    /// Clue or hostage `k`: guarded, ready or done.
    fn state(&self, world: &mut World, k: usize) -> u8 {
        let c = &self.clues[k];
        if c.found {
            DONE
        } else if c.hostage.is_some_and(|(room, _)| {
            self.rooms
                .get(room)
                .is_some_and(|r| !subdued(world, r.spot))
        }) {
            GUARDED
        } else {
            READY
        }
    }

    /// A hostage untied: they thank their hero, then walk out of the
    /// building by its route map and a few steps down the street.
    fn set_free(&mut self, world: &mut World, tick: u64, id: u32) {
        crate::incidents::release_bystanders(world, tick, id);
        let (floors, c) = (self.floors.clone(), self.case);
        let solids = world.resource::<crate::Solids>().0.clone();
        let mut q = world.query::<(Entity, &Motion, &crate::incidents::Bystander)>();
        let Some((e, from)) = q
            .iter(world)
            .find(|(_, _, b)| b.incident == id)
            .map(|(e, m, _)| (e, m.0.pos))
        else {
            return;
        };
        let b = &floors.buildings[c.building];
        let door = b.to_town(b.graph.front());
        let out = DVec2::new(door.x - b.pos.x, door.z - b.pos.y).normalize_or(DVec2::X) * 8.0;
        let mut way = walk_out(&solids, &floors, from, door);
        way.push(door + DVec3::new(out.x, 0.0, out.y));
        world.entity_mut(e).insert(crate::incidents::WayOut(way));
    }

    /// Rooms cleared, of the rooms held (the boss's not counted).
    fn cleared(&self, world: &mut World) -> (u8, u8) {
        let n = self.rooms.iter().filter(|r| beaten(world, r.spot)).count();
        (n as u8, self.rooms.len() as u8)
    }
}

/// The way from `from` in a building to its door, every flat stretch
/// round the furniture.
fn walk_out(
    solids: &crate::collision::Obstacles,
    floors: &Floors,
    from: DVec3,
    door: DVec3,
) -> Vec<DVec3> {
    let Some(legs) = floors.route(from, door, false) else {
        return vec![door];
    };
    let mut out = Vec::new();
    let mut at = from;
    for leg in legs {
        let flat = (leg.to.y - at.y).abs() < 0.3;
        if flat && !crate::nav::walkable_line(solids, at, leg.to) {
            if let Some(p) = crate::nav::find_path(solids, at, leg.to) {
                out.extend(p);
            }
        }
        out.push(leg.to);
        at = leg.to;
    }
    if out.last().is_none_or(|p| p.distance(door) > 0.1) {
        out.push(door);
    }
    out
}

/// Is a room's group all down (cuffed, or carried off by the drone) or gone?
fn subdued(world: &mut World, spot: usize) -> bool {
    beaten(world, spot)
        || (world
            .resource::<Fights>()
            .spots
            .get(spot)
            .is_some_and(|s| s.groups > 0)
            && world
                .query::<(&combat::Enemy, &Fighter)>()
                .iter(world)
                .filter(|(e, _)| e.spot == spot)
                .all(|(_, f)| {
                    !matches!(f.status, hw_net::Status::Standing | hw_net::Status::Knocked)
                }))
}

/// Is a room's group all beaten (or gone)?
fn beaten(world: &World, spot: usize) -> bool {
    let f = world.resource::<Fights>();
    f.spots
        .get(spot)
        .is_some_and(|s| s.groups > 0 && s.members == 0)
}

fn flat(a: DVec3, b: DVec3) -> f64 {
    DVec2::new(a.x - b.x, a.z - b.z).length()
}

/// Is there open floor round `p` (a group can stand and fight there)?
fn open_round(solids: &crate::collision::Obstacles, p: DVec2, y: f64, r: f64) -> bool {
    room_to_stand(solids, p, y)
        && (0..8).all(|k| {
            let d = hw_core::det::dir2(std::f64::consts::TAU * k as f64 / 8.0);
            room_to_stand(solids, p + d * r, y)
        })
}

/// The open floor nearest a room's middle, in the town's frame.
fn open_middle(
    solids: &crate::collision::Obstacles,
    b: &crate::floornav::Building,
    rect: &hw_gen::geom::Rect,
    y: f64,
) -> Option<DVec3> {
    let c = rect.center();
    candidates(rect)
        .into_iter()
        .filter_map(|p| {
            let t = b.to_town(DVec3::new(p.x, y, p.y));
            open_round(solids, DVec2::new(t.x, t.z), t.y, 1.8).then_some((p.distance(c), t))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|x| x.1)
}

/// Open floor in a room as far as can be from `from` (a clue tucked away
/// from where the group stands).
fn far_corner(
    solids: &crate::collision::Obstacles,
    b: &crate::floornav::Building,
    rect: &hw_gen::geom::Rect,
    y: f64,
    from: DVec3,
) -> Option<DVec3> {
    candidates(rect)
        .into_iter()
        .filter_map(|p| {
            let t = b.to_town(DVec3::new(p.x, y, p.y));
            open_round(solids, DVec2::new(t.x, t.z), t.y, 0.9).then_some((flat(t, from), t))
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|x| x.1)
}

/// Points on a metre grid in a room, two metres in from its walls.
fn candidates(rect: &hw_gen::geom::Rect) -> Vec<DVec2> {
    let r = rect.inset(2.0);
    let mut out = Vec::new();
    let mut x = r.min.x;
    while x <= r.max.x {
        let mut z = r.min.y;
        while z <= r.max.y {
            out.push(DVec2::new(x, z));
            z += 1.0;
        }
        x += 1.0;
    }
    out
}

struct Seen {
    id: NetId,
    at: DVec3,
    level: u8,
    /// Holding the help key, on their feet.
    help: bool,
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
