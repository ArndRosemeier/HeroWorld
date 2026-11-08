//! Incidents: trouble that comes to the heroes. Every so often something
//! starts within a few hundred metres of a hero: a mugging, a robbery at a
//! shop, a mutant rampage, a breakout from one of the mad scientist's labs.
//! Each is a group of villains standing where it happens; beating them all
//! resolves it (karma for everyone near), and one nobody answers ends after
//! a few minutes (they get away).
//!
//! The spawn rule (DESIGN.md): two to four open at once within about 800 m
//! of the heroes, at least a minute apart. Which and where come from the
//! world's seed and a running count, so a host restarted from the same
//! save sees the same sequence.

use crate::combat::{self, EnemyKind, Fighter, Fights, Spot};
use crate::progress::{self, Affinity};
use crate::{Hero, Motion, Net, Solids, GROUND_Y, TICK_HZ};
use bevy_ecs::prelude::*;
use glam::{DVec2, DVec3};
use hw_core::{NetId, Seed};
use hw_gen::building::BuildingUse;
use hw_gen::urban::world::TownWorld;
use hw_net::ToClient;
use std::sync::Arc;

/// At least this long between two incidents starting (s).
pub const GAP_SECONDS: f64 = 60.0;
/// The first one starts this long after the world does (s).
pub const FIRST_SECONDS: f64 = 20.0;
/// An incident nobody answers ends after this long (s).
pub const TIMEOUT_SECONDS: f64 = 240.0;
/// A hero this near has answered it (m).
pub const ANSWER: f64 = 60.0;
/// Heroes this near share the karma when it is resolved (m).
pub const REWARD: f64 = 120.0;
/// Incidents start this far from a hero (m).
pub const NEAREST: f64 = 150.0;
pub const FARTHEST: f64 = 800.0;

/// What kind of trouble.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IncidentKind {
    /// Two thugs robbing someone in the street.
    Mugging,
    /// A gang holding up a shop.
    Robbery,
    /// Mutant brutes smashing up a street.
    Rampage,
    /// The mad scientist's technicians loose from a lab.
    Breakout,
}

impl IncidentKind {
    pub const ALL: [IncidentKind; 4] = [
        IncidentKind::Mugging,
        IncidentKind::Robbery,
        IncidentKind::Rampage,
        IncidentKind::Breakout,
    ];

    pub fn code(self) -> u8 {
        self as u8
    }

    pub fn from_code(c: u8) -> Option<Self> {
        Self::ALL.get(c as usize).copied()
    }

    /// What the wrist radio calls it.
    pub fn name(self) -> &'static str {
        match self {
            IncidentKind::Mugging => "Mugging",
            IncidentKind::Robbery => "Robbery in progress",
            IncidentKind::Rampage => "Mutant rampage",
            IncidentKind::Breakout => "Lab breakout",
        }
    }

    /// The group: how many, which kind only, levels above the team.
    fn group(self) -> (u32, Option<EnemyKind>, u8) {
        match self {
            IncidentKind::Mugging => (2, Some(EnemyKind::Brute), 0),
            IncidentKind::Robbery => (4, None, 1),
            IncidentKind::Rampage => (3, Some(EnemyKind::Brute), 2),
            IncidentKind::Breakout => (5, Some(EnemyKind::Technician), 2),
        }
    }

    /// Karma for resolving it, on top of the villains' own.
    fn bonus(self) -> f32 {
        match self {
            IncidentKind::Mugging => 1.0,
            IncidentKind::Robbery => 1.5,
            IncidentKind::Rampage => 2.0,
            IncidentKind::Breakout => 2.5,
        }
    }
}

/// How an incident stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Open,
    Resolved,
    TimedOut,
}

impl State {
    fn code(self) -> u8 {
        match self {
            State::Open => 0,
            State::Resolved => 1,
            State::TimedOut => 2,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Incident {
    pub id: u32,
    pub kind: IncidentKind,
    pub at: DVec3,
    /// Its villains' fight spot.
    pub spot: usize,
    pub level: u8,
    pub started: u64,
    /// Unanswered, it ends at this tick.
    pub expires: u64,
    pub answered: bool,
    pub state: State,
}

impl Incident {
    fn news(&self) -> ToClient {
        ToClient::Incident {
            id: self.id,
            kind: self.kind.code(),
            at: self.at.as_vec3().to_array(),
            level: self.level,
            state: self.state.code(),
        }
    }
}

/// The town's incidents.
#[derive(Default)]
pub struct Incidents {
    town: Option<Arc<TownWorld>>,
    /// Open ones.
    pub open: Vec<Incident>,
    /// How many have started (seeds the next).
    pub count: u32,
    last_start: Option<u64>,
    /// Seconds before the first one (default [`FIRST_SECONDS`]).
    pub first_after: f64,
    /// Spots of incidents that ended, free to use again.
    free: Vec<usize>,
    news: Vec<ToClient>,
}

/// Two to four open at once: more with more heroes.
pub fn wanted(heroes: usize) -> usize {
    (1 + heroes).clamp(2, 4)
}

fn ticks(seconds: f64) -> u64 {
    (seconds * TICK_HZ as f64).round() as u64
}

impl Incidents {
    pub fn new(town: Arc<TownWorld>) -> Self {
        Self {
            town: Some(town),
            first_after: FIRST_SECONDS,
            ..Default::default()
        }
    }

    /// Once a second: start, answer, resolve and time out incidents.
    pub(crate) fn step(&mut self, world: &mut World, tick: u64) {
        if self.town.is_none() || !tick.is_multiple_of(TICK_HZ as u64) {
            return;
        }
        let mut heroes: Vec<(NetId, DVec3, u8)> = world
            .query::<(&Hero, &Net, &Motion, &Fighter)>()
            .iter(world)
            .map(|(_, n, m, f)| (n.0, m.0.pos, f.level))
            .collect();
        heroes.sort_by_key(|h| h.0);
        self.settle(world, tick, &heroes);
        if heroes.is_empty() || self.open.len() >= wanted(heroes.len()) {
            return;
        }
        let due = match self.last_start {
            None => tick >= ticks(self.first_after),
            Some(t) => tick >= t + ticks(GAP_SECONDS),
        };
        if due {
            self.start(world, tick, &heroes);
        }
    }

    /// Answered, resolved or timed out.
    fn settle(&mut self, world: &mut World, tick: u64, heroes: &[(NetId, DVec3, u8)]) {
        let mut ended = Vec::new();
        for (k, inc) in self.open.iter_mut().enumerate() {
            if heroes.iter().any(|h| h.1.distance(inc.at) < ANSWER) {
                inc.answered = true;
            }
            let spot = &world.resource::<Fights>().spots[inc.spot];
            if spot.groups > 0 && spot.members == 0 {
                inc.state = State::Resolved;
                ended.push(k);
            } else if tick >= inc.expires && !inc.answered {
                inc.state = State::TimedOut;
                ended.push(k);
            } else if inc.answered && heroes.iter().all(|h| h.1.distance(inc.at) > combat::SLEEP) {
                // Answered, then left behind: they get away all the same.
                inc.state = State::TimedOut;
                ended.push(k);
            }
        }
        for k in ended.into_iter().rev() {
            let inc = self.open.remove(k);
            match inc.state {
                State::Resolved => self.reward(world, &inc, heroes),
                _ => Self::send_home(world, inc.spot),
            }
            self.free.push(inc.spot);
            self.news.push(inc.news());
        }
    }

    fn reward(&self, world: &mut World, inc: &Incident, heroes: &[(NetId, DVec3, u8)]) {
        let (size, only, _) = inc.kind.group();
        let base = combat::enemy_spec(only.unwrap_or(EnemyKind::Brute)).karma;
        let mut f = world.resource_mut::<Fights>();
        for h in heroes.iter().filter(|h| h.1.distance(inc.at) < REWARD) {
            let one = progress::defeat_karma(base, inc.level, h.2) as f32;
            f.rewards.push(combat::Reward {
                hero: h.0,
                karma: (one * size as f32 * inc.kind.bonus()).round() as u32,
                deed: Some(Affinity::Justice),
                lean: 0,
            });
        }
    }

    /// The villains of an incident that ended unanswered leave.
    fn send_home(world: &mut World, spot: usize) {
        let mut gone: Vec<(NetId, Entity)> = world
            .query::<(Entity, &Net, &combat::Enemy)>()
            .iter(world)
            .filter(|(_, _, en)| en.spot == spot)
            .map(|(e, n, _)| (n.0, e))
            .collect();
        gone.sort_by_key(|g| g.0);
        for (id, e) in gone {
            world.resource_mut::<Fights>().paths.remove(&id);
            world.despawn(e);
        }
        let mut f = world.resource_mut::<Fights>();
        f.spots[spot].members = 0;
        f.spots[spot].respawn_at = None;
    }

    /// Starts one near a hero.
    fn start(&mut self, world: &mut World, tick: u64, heroes: &[(NetId, DVec3, u8)]) {
        let Some(town) = self.town.clone() else {
            return;
        };
        let seed = Seed(world.resource::<Fights>().seed)
            .child("incidents")
            .child(self.count as u64);
        self.count += 1;
        self.last_start = Some(tick);
        let mut rng = seed.rng();
        let hero = rng.pick(heroes).1;
        let me = DVec2::new(hero.x, hero.z);
        let ring = |p: DVec2| (NEAREST..FARTHEST).contains(&p.distance(me));
        let quarter = town.town.dystopia.as_ref();
        let held = |p: DVec2| quarter.is_some_and(|d| d.contains(p));
        // Labs near enough for a breakout, shops for a robbery.
        let fronts = |usage: BuildingUse| -> Vec<(DVec2, DVec2)> {
            town.buildings
                .iter()
                .filter(|b| b.site.usage == usage)
                .map(|b| {
                    let s = &b.site;
                    let front = s.origin + s.along * s.size.x * 0.5 - s.inward * 3.0;
                    (front, -s.inward)
                })
                .filter(|(p, _)| ring(*p))
                .collect()
        };
        let labs = fronts(BuildingUse::Lab);
        let shops: Vec<_> = fronts(BuildingUse::ShopsAndOffices)
            .into_iter()
            .filter(|(p, _)| !held(*p))
            .collect();
        let streets: Vec<(DVec2, DVec2)> = town
            .roads
            .iter()
            .filter(|r| !r.main)
            .flat_map(|r| r.pts.windows(2).map(|w| (w[0], (w[1] - w[0]).perp())))
            .filter(|(p, _)| ring(*p) && !held(*p))
            .collect();
        let mut kinds: Vec<(IncidentKind, f64)> = vec![
            (IncidentKind::Mugging, 0.35),
            (IncidentKind::Robbery, 0.3),
            (IncidentKind::Rampage, 0.2),
            (IncidentKind::Breakout, 0.15),
        ];
        kinds.retain(|(k, _)| match k {
            IncidentKind::Breakout => !labs.is_empty(),
            IncidentKind::Robbery => !shops.is_empty(),
            _ => !streets.is_empty(),
        });
        let total: f64 = kinds.iter().map(|k| k.1).sum();
        if total <= 0.0 {
            return;
        }
        let mut roll = rng.range(0.0, total);
        let mut kind = kinds[0].0;
        for (k, w) in &kinds {
            if roll < *w {
                kind = *k;
                break;
            }
            roll -= w;
        }
        let places = match kind {
            IncidentKind::Breakout => &labs,
            IncidentKind::Robbery => &shops,
            _ => &streets,
        };
        let (p, face) = *rng.pick(places);
        // Not on top of another one.
        if self
            .open
            .iter()
            .any(|i| DVec2::new(i.at.x, i.at.z).distance(p) < 60.0)
        {
            return;
        }
        let y = world.resource::<Solids>().0.ground_at(p).max(GROUND_Y);
        let at = DVec3::new(p.x, y, p.y);
        // Facing the hero who will come.
        let facing = (me - p).normalize_or(face.normalize_or(DVec2::X));
        let (size, only, extra) = kind.group();
        let spot = Spot {
            extra,
            size: Some(size),
            only,
            once: true,
            ..Spot::corner(at, facing)
        };
        let index = {
            let mut f = world.resource_mut::<Fights>();
            match self.free.pop() {
                Some(i) => {
                    // Keep the slot's count of groups: it varies the roster.
                    let groups = f.spots[i].groups;
                    f.spots[i] = Spot { groups, ..spot };
                    i
                }
                None => {
                    f.spots.push(spot);
                    f.spots.len() - 1
                }
            }
        };
        combat::spawn_group(world, tick, index, heroes.len());
        let level = world
            .resource::<Fights>()
            .team_level
            .saturating_add(extra)
            .min(progress::MAX_LEVEL);
        let inc = Incident {
            id: self.count,
            kind,
            at,
            spot: index,
            level,
            started: tick,
            expires: tick + ticks(TIMEOUT_SECONDS),
            answered: false,
            state: State::Open,
        };
        self.news.push(inc.news());
        self.open.push(inc);
    }

    /// Every open incident, for a player who joins.
    pub fn snapshot(&self) -> Vec<ToClient> {
        self.open.iter().map(Incident::news).collect()
    }

    pub fn take_news(&mut self) -> Vec<ToClient> {
        std::mem::take(&mut self.news)
    }
}
