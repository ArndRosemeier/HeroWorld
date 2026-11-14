//! Street crime on the server. The crimes themselves are planned from the
//! seed and the clock (see [`crate::streetlife::crime`]), so every machine
//! draws the same ones without a word sent. The server does what makes
//! them matter:
//!
//! - **Stepping in.** A hero who comes close while one is going on steps
//!   in: its crooks become real villains where they stand (in the looks
//!   they were drawn in), they turn on the hero, and the victim cowers
//!   until it is over and thanks them. Every client is told
//!   ([`ToClient::Crime`]) and stops drawing the planned crime.
//! - **Smashed things stay smashed.** A lamp the mad scientist's brutes
//!   smash breaks for real ([`crate::props::PropDamage::smash`]): it stays
//!   dark until it is mended, off camera, like anything a fight breaks.
//! - **People notice.** Passers-by stop and stare at a crime going on
//!   near a hero (a warning alarm), and run from a fight once a hero
//!   steps in.
//!
//! The crimes near the heroes are worked out on a worker thread: asked
//! for at each whole second and taken in at the next one, waiting for the
//! answer if it is late, so what the server does never depends on how
//! quickly a thread ran.

use crate::alarms::{AlarmKind, Alarms};
use crate::combat::{self, EnemyKind, Fighter, Fights, Spot};
use crate::incidents;
use crate::progress;
use crate::props::PropDamage;
use crate::streetlife::crime::{Crime, CrimePerson, CrimeRole, Loot};
use crate::streetlife::StreetLife;
use crate::{Hero, Motion, Net, DT, TICK_HZ};
use bevy_ecs::prelude::*;
use glam::{DVec2, DVec3};
use hw_core::NetId;
use hw_gen::urban::area::Area;
use hw_net::{FxKind, ToClient};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

/// A hero this near one of a crime's crooks (or its victim) while it is
/// going on steps in (m).
pub const STEP_IN: f64 = 7.0;
/// Crimes are looked for this far round each hero (m).
const LOOK: f64 = 110.0;
/// Passers-by notice a crime near a hero within this distance of them (m).
const NOTICE: f64 = 150.0;
/// A smashed thing breaks for real only with a hero this near (m): out of
/// everyone's sight it does not matter.
const SMASH_SEEN: f64 = 220.0;
/// Once stepped in, the crooks provoke at this reach (m).
const PROVOKE: f64 = STEP_IN + 8.0;
/// Stepped in on and then left: with no hero this near (m) for
/// [`LEFT_SECONDS`], the crooks get away (out of sight).
const LEFT_REACH: f64 = 160.0;
const LEFT_SECONDS: f64 = 20.0;
/// Karma for stopping one, as a share of beating its crooks.
const BONUS: f32 = 0.6;
/// Bystander numbers of street crimes (apart from incidents').
const VICTIMS: u32 = 0x8000_0000;

/// A crime a hero stepped in on.
#[derive(Clone, Debug)]
struct Taken {
    spot: usize,
    crooks: u32,
    kind: EnemyKind,
    level: u8,
    at: DVec3,
    victim: u32,
    alarm: u32,
    /// Since when no hero has been near.
    left: Option<u64>,
}

/// A question to the worker: the crimes round these points at this time.
struct Ask {
    at: Vec<DVec2>,
    time: f64,
}

pub struct Crimes {
    life: Option<Arc<StreetLife>>,
    ask: Option<Sender<Ask>>,
    answers: Option<Receiver<Vec<Arc<Crime>>>>,
    asked: bool,
    /// The crimes round the heroes, from the last answer.
    near: Vec<Arc<Crime>>,
    taken: BTreeMap<(u64, i64), Taken>,
    /// Crimes over (stopped or got away), still remembered while they
    /// could be drawn: a client must not draw them again.
    over: BTreeMap<(u64, i64), (f64, u8)>,
    /// Things already smashed (by crime).
    smashed: BTreeSet<(u64, i64)>,
    /// Spots of stepped-in crimes that are over, free to use again.
    free: Vec<usize>,
    count: u32,
    last_time: f64,
    news: Vec<ToClient>,
}

impl Default for Crimes {
    fn default() -> Self {
        Self {
            life: None,
            ask: None,
            answers: None,
            asked: false,
            near: Vec::new(),
            taken: BTreeMap::new(),
            over: BTreeMap::new(),
            smashed: BTreeSet::new(),
            free: Vec::new(),
            count: 0,
            last_time: 0.0,
            news: Vec::new(),
        }
    }
}

fn ticks(seconds: f64) -> u64 {
    (seconds * TICK_HZ as f64).round() as u64
}

fn loot_code(l: Loot) -> u16 {
    use hw_net::protocol::loot;
    match l {
        Loot::Handbag => loot::HANDBAG,
        Loot::Cash => loot::CASH,
        Loot::Envelope => loot::ENVELOPE,
        Loot::SprayCan => loot::SPRAY_CAN,
    }
}

impl Crimes {
    /// The street crime of the town square `area` (a new square: the
    /// crimes of the old one are forgotten, stepped-in ones play out).
    pub fn set_area(&mut self, area: Arc<Area>) {
        self.life = Some(Arc::new(StreetLife::new(area)));
        // A new worker for the new square; the old one ends with its
        // channel.
        self.ask = None;
        self.answers = None;
        self.asked = false;
        self.near.clear();
    }

    /// Every tick: steps in, smashes, alarms; at each whole second the
    /// crimes round the heroes are asked for anew.
    pub(crate) fn step(
        &mut self,
        world: &mut World,
        tick: u64,
        street: &mut PropDamage,
        alarms: &mut Alarms,
    ) {
        let Some(life) = self.life.clone() else {
            return;
        };
        let now = tick as f64 * DT;
        let mut heroes: Vec<(NetId, DVec3, u8)> = world
            .query::<(&Hero, &Net, &Motion, &Fighter)>()
            .iter(world)
            .filter(|(_, _, _, f)| f.standing())
            .map(|(_, n, m, f)| (n.0, m.0.pos, f.level))
            .collect();
        heroes.sort_by_key(|h| h.0);
        // Beaten crooks end the crime at once (thanks, karma), checked a
        // few times a second.
        if tick.is_multiple_of(6) && !self.taken.is_empty() {
            self.settle(world, tick, &heroes, alarms);
        }
        if tick.is_multiple_of(TICK_HZ as u64) {
            self.take_answer();
            self.ask_worker(&life, &heroes, now);
            self.notice(&heroes, now, tick, alarms);
            let gone = now - 60.0;
            self.over.retain(|_, (until, _)| *until > gone);
            self.smashed
                .retain(|k| self.near.iter().any(|c| c.key() == *k));
        }
        // Smashing, as it happens (between the last tick and this one).
        for c in &self.near {
            let Some(at) = c.smash else {
                continue;
            };
            if !(self.last_time < at && at <= now)
                || self.taken.contains_key(&c.key())
                || self.over.contains_key(&c.key())
                || !self.smashed.insert(c.key())
            {
                continue;
            }
            let seen = heroes
                .iter()
                .any(|h| DVec2::new(h.1.x, h.1.z).distance(c.place.at) < SMASH_SEEN);
            if let (true, Some((id, _))) = (seen, c.place.prop) {
                let from = c.place.spots.first().copied().unwrap_or(c.place.at);
                street.smash(id, from, tick);
            }
        }
        self.last_time = now;
        // Stepping in: a few times a second is plenty.
        if !tick.is_multiple_of(6) {
            return;
        }
        let mut stepped: Vec<(Arc<Crime>, NetId)> = Vec::new();
        for c in &self.near {
            if !c.hot(now) || self.taken.contains_key(&c.key()) || self.over.contains_key(&c.key())
            {
                continue;
            }
            let folk = c.people(&life, now);
            let hero = heroes.iter().find(|h| {
                folk.iter()
                    .any(|p| p.pos.distance(h.1) < STEP_IN && in_reach(p, h.1))
            });
            if let Some(h) = hero {
                stepped.push((c.clone(), h.0));
            }
        }
        for (c, hero) in stepped {
            self.step_in(world, tick, &life, &c, hero, &heroes, alarms);
        }
    }

    /// Takes in the answer asked for a second ago, waiting for it if the
    /// worker is late.
    fn take_answer(&mut self) {
        if !self.asked {
            return;
        }
        self.asked = false;
        if let Some(r) = &self.answers {
            match r.recv() {
                Ok(near) => self.near = near,
                Err(_) => {
                    // The worker is gone (it panicked): ask a new one.
                    self.ask = None;
                    self.answers = None;
                }
            }
        }
    }

    fn ask_worker(&mut self, life: &Arc<StreetLife>, heroes: &[(NetId, DVec3, u8)], now: f64) {
        if heroes.is_empty() {
            self.near.clear();
            return;
        }
        if self.ask.is_none() {
            let (ask, asks) = channel::<Ask>();
            let (tell, answers) = channel();
            let life = life.clone();
            let spawned = std::thread::Builder::new()
                .name("hw-crimes".into())
                .spawn(move || {
                    while let Ok(a) = asks.recv() {
                        let mut all: Vec<Arc<Crime>> = Vec::new();
                        for p in &a.at {
                            for c in life.crimes_near(*p, LOOK, a.time, 3.0) {
                                if !all.iter().any(|o| o.key() == c.key()) {
                                    all.push(c);
                                }
                            }
                        }
                        all.sort_by_key(|c| c.key());
                        if tell.send(all).is_err() {
                            return;
                        }
                    }
                });
            if spawned.is_err() {
                return;
            }
            self.ask = Some(ask);
            self.answers = Some(answers);
        }
        let at = heroes.iter().map(|h| DVec2::new(h.1.x, h.1.z)).collect();
        if let Some(ask) = &self.ask {
            // For the second the answer will be used in.
            self.asked = ask
                .send(Ask {
                    at,
                    time: now + 1.0,
                })
                .is_ok();
        }
    }

    /// Passers-by near a crime going on stop and stare.
    fn notice(&self, heroes: &[(NetId, DVec3, u8)], now: f64, tick: u64, alarms: &mut Alarms) {
        for c in &self.near {
            if !(c.on <= now && now < c.done + 4.0) || self.taken.contains_key(&c.key()) {
                continue;
            }
            let at = DVec3::new(c.place.at.x, c.place.y, c.place.at.y);
            if heroes.iter().any(|h| h.1.distance(at) < NOTICE) {
                alarms.raise_or_refresh(AlarmKind::Warning, at, tick, TICK_HZ);
            }
        }
    }

    /// A hero stepped in: the crooks are real now, and turn on them.
    #[allow(clippy::too_many_arguments)]
    fn step_in(
        &mut self,
        world: &mut World,
        tick: u64,
        life: &StreetLife,
        c: &Crime,
        hero: NetId,
        heroes: &[(NetId, DVec3, u8)],
        alarms: &mut Alarms,
    ) {
        let now = tick as f64 * DT;
        let folk = c.people(life, now);
        let crooks: Vec<&CrimePerson> = folk
            .iter()
            .filter(|p| matches!(p.role, CrimeRole::Crook(..)))
            .collect();
        if crooks.is_empty() {
            return;
        }
        let at = DVec3::new(c.place.at.x, c.place.y, c.place.at.y);
        let hero_at = heroes.iter().find(|h| h.0 == hero).map_or(at, |h| h.1);
        let facing = DVec2::new(hero_at.x - at.x, hero_at.z - at.z).normalize_or(c.place.along);
        let kind = c.crook();
        let spot = Spot {
            size: Some(crooks.len() as u32),
            only: Some(kind),
            once: true,
            provoke: Some(PROVOKE),
            ..Spot::corner(at, facing)
        };
        let index = {
            let mut f = world.resource_mut::<Fights>();
            match combat::take_free_spot(&mut self.free, &f.spots) {
                Some(i) => {
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
        let level = world.resource::<Fights>().team_level;
        let mut ids = Vec::new();
        let mut fx = Vec::new();
        for p in &crooks {
            let CrimeRole::Crook(k, look, _) = p.role else {
                continue;
            };
            let id = combat::spawn_enemy(world, tick, k, level, index, p.pos, heroes.len());
            ids.push(id);
            if let Some(l) = p.holding {
                let mut e = combat::event(tick, FxKind::Loot, id, id, p.pos);
                e.amount = loot_code(l);
                fx.push(e);
            }
            let mut q = world.query::<(&Net, &mut combat::Enemy, &mut Fighter)>();
            for (n, mut en, mut f) in q.iter_mut(world) {
                if n.0 == id {
                    en.look = Some(look);
                    f.target = Some(hero);
                }
            }
        }
        {
            let mut f = world.resource_mut::<Fights>();
            let s = &mut f.spots[index];
            s.members = ids.len() as u32;
            s.groups += 1;
            f.events.extend(fx);
        }
        self.count += 1;
        let victim = VICTIMS | self.count;
        if let Some(v) = folk.iter().find(|p| matches!(p.role, CrimeRole::Victim(_))) {
            let CrimeRole::Victim(look) = v.role else {
                unreachable!()
            };
            // Facing the crooks, cowering until it is over.
            let to = crooks[0].pos - v.pos;
            let face = DVec2::new(to.x, to.z).normalize_or(v.facing);
            incidents::spawn_bystander(world, tick, victim, v.pos, face, Some(look as u8));
        }
        let alarm = alarms.raise(AlarmKind::Mugging, at, tick, TICK_HZ);
        self.taken.insert(
            c.key(),
            Taken {
                spot: index,
                crooks: ids.len() as u32,
                kind,
                level,
                at,
                victim,
                alarm,
                left: None,
            },
        );
        self.news.push(ToClient::Crime {
            place: c.place.key,
            slot: c.slot,
            state: 1,
        });
    }

    /// Stepped-in crimes: stopped once their crooks are beaten (karma for
    /// the heroes there, thanks from the victim), or gone once the heroes
    /// left them.
    fn settle(
        &mut self,
        world: &mut World,
        tick: u64,
        heroes: &[(NetId, DVec3, u8)],
        alarms: &mut Alarms,
    ) {
        let now = tick as f64 * DT;
        let mut ended: Vec<((u64, i64), u8)> = Vec::new();
        let crooks = incidents::crooks_of(world);
        for (key, t) in self.taken.iter_mut() {
            let beaten = world.resource::<Fights>().spots[t.spot].members == 0
                || incidents::all_down(&crooks, t.spot);
            if beaten {
                ended.push((*key, 2));
                continue;
            }
            let near = heroes.iter().any(|h| h.1.distance(t.at) < LEFT_REACH);
            if near {
                t.left = None;
                alarms.refresh(t.alarm, t.at, tick, TICK_HZ);
            } else if t.left.is_none() {
                t.left = Some(tick);
            } else if t.left.is_some_and(|l| tick >= l + ticks(LEFT_SECONDS)) {
                ended.push((*key, 3));
            }
        }
        for (key, state) in ended {
            let Some(t) = self.taken.remove(&key) else {
                continue;
            };
            alarms.end(t.alarm);
            incidents::release_bystanders(world, tick, t.victim);
            if state == 2 {
                let base = combat::enemy_spec(t.kind).karma;
                let mut f = world.resource_mut::<Fights>();
                for h in heroes
                    .iter()
                    .filter(|h| h.1.distance(t.at) < incidents::REWARD)
                {
                    let one = progress::defeat_karma(base, t.level, h.2) as f32;
                    let karma = (one * t.crooks as f32 * BONUS).round().max(1.0) as u32;
                    f.rewards.push(combat::Reward { hero: h.0, karma });
                }
            } else {
                // Off out of sight with whatever they took.
                let mut gone: Vec<(NetId, Entity)> = world
                    .query::<(Entity, &Net, &combat::Enemy)>()
                    .iter(world)
                    .filter(|(_, _, en)| en.spot == t.spot)
                    .map(|(e, n, _)| (n.0, e))
                    .collect();
                gone.sort_by_key(|g| g.0);
                for (id, e) in gone {
                    world.resource_mut::<Fights>().paths.remove(&id);
                    world.despawn(e);
                }
                let mut f = world.resource_mut::<Fights>();
                f.spots[t.spot].members = 0;
            }
            self.free.push(t.spot);
            self.over.insert(key, (now + 600.0, state));
            self.news.push(ToClient::Crime {
                place: key.0,
                slot: key.1,
                state,
            });
        }
    }

    /// The crimes heroes have stepped in on that are still on, and those
    /// over a little while: for a player who joins.
    pub fn snapshot(&self) -> Vec<ToClient> {
        self.taken
            .keys()
            .map(|k| (k, 1))
            .chain(self.over.iter().map(|(k, (_, s))| (k, *s)))
            .map(|(k, state)| ToClient::Crime {
                place: k.0,
                slot: k.1,
                state,
            })
            .collect()
    }

    pub fn take_news(&mut self) -> Vec<ToClient> {
        std::mem::take(&mut self.news)
    }

    /// How many crimes are stepped in on now (tests).
    pub fn stepped_in(&self) -> usize {
        self.taken.len()
    }

    /// The crimes round the heroes now (tests and views).
    pub fn near(&self) -> &[Arc<Crime>] {
        &self.near
    }

    /// The street life the crimes come from.
    pub fn life(&self) -> Option<&Arc<StreetLife>> {
        self.life.as_ref()
    }
}

/// Is someone of a crime at `p` within a hero's reach at `hero` (not a
/// storey up or down)?
fn in_reach(p: &CrimePerson, hero: DVec3) -> bool {
    (p.pos.y - hero.y).abs() < 4.0
}
