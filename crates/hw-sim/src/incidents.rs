//! Incidents: trouble that comes to the heroes. Every so often something
//! starts within a few hundred metres of a hero: a mugging, a robbery at a
//! shop, a mutant rampage, a breakout from one of the mad scientist's labs,
//! a car crash with people trapped, a fire with people inside.
//!
//! A fight is a group of villains standing where it happens; beating them
//! all resolves it (karma for everyone near). A rescue is people to get
//! out: a hero holds the help key (the revive key) next to each in turn
//! until they are free (Guardian karma for each). The last one out is hurt:
//! the hero who freed them carries them to the ambulance waiting down the
//! street, and only then is the rescue done. A carrier knocked down drops
//! them; anyone can pick them up again with the same key. One nobody
//! answers ends after a few minutes (the crooks get away, or the fire
//! brigade comes).
//!
//! The spawn rule (DESIGN.md): two to four open at once within about 800 m
//! of the heroes, at least a minute apart. Which and where come from the
//! world's seed and a running count, so a host restarted from the same
//! save sees the same sequence.

use crate::combat::{self, EnemyKind, Fighter, Fights, Spot};
use crate::progress::{self, Affinity};
use crate::{Control, Hero, Motion, Net, Solids, DT, GROUND_Y, TICK_HZ};
use bevy_ecs::prelude::*;
use glam::{DVec2, DVec3};
use hw_core::{NetId, Seed};
use hw_gen::building::BuildingUse;
use hw_gen::urban::area::Area;
use hw_gen::urban::landmarks::LandmarkKind;
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
/// A colossus nobody answers goes home after this long (s).
pub const COLOSSUS_TIMEOUT: f64 = 600.0;
/// The colossus comes out at most this often (s), and not before the team
/// reaches this level.
pub const COLOSSUS_GAP: f64 = 1500.0;
pub const COLOSSUS_LEVEL: u8 = 3;
/// A hero this near a trapped person can help them out (m).
pub const RESCUE_REACH: f64 = 3.5;
/// The ambulance parks this far from junctions (m).
const MEDIC_CLEAR: f64 = 15.0;
/// Carrying the hurt this near the medic hands them over (m).
pub const MEDIC_REACH: f64 = 5.0;
/// Seconds of holding the help key to lift someone lying hurt.
pub const LIFT_SECONDS: f64 = 1.0;
/// The ambulance waits about this far from a rescue (m).
const MEDIC_AWAY: f64 = 50.0;

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
    /// Two cars smashed together, people trapped inside.
    CarCrash,
    /// A house on fire, people inside.
    Fire,
    /// The mad scientist's walking tank marches out of his quarter.
    Colossus,
    /// A building brought down in a fight (by heroes or the colossus),
    /// people trapped in the rubble. Never rolled: it follows a collapse.
    Collapse,
}

impl IncidentKind {
    pub const ALL: [IncidentKind; 8] = [
        IncidentKind::Mugging,
        IncidentKind::Robbery,
        IncidentKind::Rampage,
        IncidentKind::Breakout,
        IncidentKind::CarCrash,
        IncidentKind::Fire,
        IncidentKind::Colossus,
        IncidentKind::Collapse,
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
            IncidentKind::CarCrash => "Car crash, people trapped",
            IncidentKind::Fire => "Fire, people inside",
            IncidentKind::Colossus => "The colossus is loose!",
            IncidentKind::Collapse => "Building collapse, people trapped",
        }
    }

    /// People to get out (none for a fight).
    pub fn trapped(self) -> u8 {
        match self {
            IncidentKind::CarCrash => 2,
            IncidentKind::Fire | IncidentKind::Collapse => 3,
            _ => 0,
        }
    }

    /// Seconds of holding the help key to get one person out.
    pub fn rescue_seconds(self) -> f64 {
        match self {
            IncidentKind::CarCrash => 4.0,
            // Digging them out.
            IncidentKind::Collapse => 5.0,
            _ => 3.0,
        }
    }

    /// The group: how many, which kind only, levels above the team.
    fn group(self) -> (u32, Option<EnemyKind>, u8) {
        match self {
            IncidentKind::Mugging => (2, Some(EnemyKind::Greaser), 0),
            IncidentKind::Robbery => (4, Some(EnemyKind::Gunsel), 1),
            IncidentKind::Rampage => (3, Some(EnemyKind::Brute), 2),
            IncidentKind::Breakout => (5, Some(EnemyKind::Technician), 2),
            IncidentKind::Colossus => (1, Some(EnemyKind::Colossus), 3),
            IncidentKind::CarCrash | IncidentKind::Fire | IncidentKind::Collapse => (0, None, 0),
        }
    }

    /// Karma for resolving it, on top of the villains' own.
    fn bonus(self) -> f32 {
        match self {
            IncidentKind::Mugging => 1.0,
            IncidentKind::Robbery => 1.5,
            IncidentKind::Rampage => 2.0,
            IncidentKind::Breakout => 2.5,
            // Per person got out.
            IncidentKind::CarCrash => 3.0,
            IncidentKind::Fire => 2.5,
            IncidentKind::Colossus => 1.5,
            IncidentKind::Collapse => 3.0,
        }
    }

    /// A hero this near has stepped in, and the crooks turn on them (m).
    /// Further off they get on with the crime, unless they would fight the
    /// hero anyway (see [`crate::factions`]).
    pub fn reach(self) -> f64 {
        match self {
            IncidentKind::Mugging => 9.0,
            IncidentKind::Robbery => 12.0,
            IncidentKind::Rampage => 16.0,
            IncidentKind::Breakout => 18.0,
            // The walking tank fights whoever comes.
            IncidentKind::Colossus => f64::INFINITY,
            IncidentKind::CarCrash | IncidentKind::Fire | IncidentKind::Collapse => 0.0,
        }
    }

    /// The lieutenant or boss in front of its group.
    fn lead(self) -> Option<EnemyKind> {
        match self {
            IncidentKind::Rampage => Some(EnemyKind::Alpha),
            IncidentKind::Breakout => Some(EnemyKind::Sentry),
            _ => None,
        }
    }

    /// Seconds it waits for heroes before it ends unanswered.
    fn timeout(self) -> f64 {
        match self {
            IncidentKind::Colossus => COLOSSUS_TIMEOUT,
            _ => TIMEOUT_SECONDS,
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

/// Where a rescue's hurt person is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hurt {
    /// On the ground, waiting to be lifted.
    Lying(DVec3),
    /// In a hero's arms.
    Carried(NetId),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Incident {
    pub id: u32,
    pub kind: IncidentKind,
    pub at: DVec3,
    /// Its villains' fight spot (none for a rescue).
    pub spot: Option<usize>,
    /// Which way it faces (a crash: across the road; a fire: out of the
    /// house).
    pub facing: DVec2,
    /// People still to get out.
    pub left: u8,
    /// Seconds a hero has been helping the next one out (or lifting the
    /// hurt one).
    pub freeing: f64,
    /// A rescue's hurt person, once out.
    pub hurt: Option<Hurt>,
    /// Where a rescue's ambulance waits, and which way it faces.
    pub medic: Option<(DVec3, DVec2)>,
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
            yaw: crate::props::yaw_to_byte(self.facing.x.atan2(self.facing.y)),
            left: self.left,
        }
    }

    /// The hurt person's news (none for a fight or before anyone is out).
    fn casualty(&self, delivered: bool) -> Option<ToClient> {
        let (medic, face) = self.medic?;
        let (state, carrier, at) = match (delivered, self.hurt) {
            (true, _) => (2, 0, medic),
            (false, Some(Hurt::Lying(p))) => (0, 0, p),
            (false, Some(Hurt::Carried(h))) => (1, h.0, self.at),
            (false, None) => return None,
        };
        Some(ToClient::Casualty {
            id: self.id,
            medic: medic.as_vec3().to_array(),
            yaw: crate::props::yaw_to_byte(face.x.atan2(face.y)),
            state,
            carrier,
            at: at.as_vec3().to_array(),
        })
    }

    /// Where the people still trapped are (the next to get out last).
    pub fn victims(&self) -> Vec<DVec3> {
        victims(self.kind, self.at, self.facing, self.left)
    }
}

/// Where the trapped people of an incident are: a crash's beside the
/// doors of its two cars, a fire's at the door and windows of the house.
/// The server and every client place them alike.
pub fn victims(kind: IncidentKind, at: DVec3, facing: DVec2, left: u8) -> Vec<DVec3> {
    let f = DVec3::new(facing.x, 0.0, facing.y).normalize_or(DVec3::X);
    let side = DVec3::new(f.z, 0.0, -f.x);
    (0..left)
        .map(|k| match kind {
            IncidentKind::CarCrash => {
                let car = if k % 2 == 0 { -1.0 } else { 1.0 };
                at + side * (2.6 * car) + f * 1.5
            }
            _ => {
                let n = kind.trapped().max(1) as f64;
                at - f * 2.2 + side * ((k as f64 - (n - 1.0) * 0.5) * 1.8)
            }
        })
        .collect()
}

/// A townsperson caught up in a crime (the one being mugged): they cower
/// while it lasts, then walk off. Not a fighter: heroes cannot target
/// townsfolk.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Bystander {
    pub incident: u32,
    /// Walking off: the way, and the tick they are gone.
    pub leaving: Option<(DVec2, u64)>,
}

/// Seconds a bystander walks off before they are gone.
pub const LEAVE_SECONDS: f64 = 12.0;
/// Their walking pace (m/s).
pub const LEAVE_SPEED: f64 = 1.4;

/// Where the one being mugged stands: in front of the muggers (who stand
/// 1.5 m ahead of the spot, facing this way).
pub fn mugged(at: DVec3, facing: DVec2) -> DVec3 {
    at + DVec3::new(facing.x, 0.0, facing.y) * 3.4
}

fn yaw_quat(dir: DVec2) -> [f32; 4] {
    glam::Quat::from_rotation_y(dir.x.atan2(dir.y) as f32).to_array()
}

fn spawn_bystander(world: &mut World, tick: u64, incident: u32, at: DVec3, facing: DVec2) {
    let id = world.resource_mut::<crate::Ids>().0.allocate();
    world.spawn((
        Net(id),
        Motion(crate::collision::Body {
            pos: at,
            vel: DVec3::ZERO,
            grounded: true,
        }),
        crate::Orientation(yaw_quat(facing)),
        Bystander {
            incident,
            leaving: None,
        },
        crate::LastChange(tick),
    ));
}

/// The crime is over: its bystanders walk off, away from where it was.
fn release_bystanders(world: &mut World, tick: u64, incident: u32) {
    for (mut b, o) in world
        .query::<(&mut Bystander, &crate::Orientation)>()
        .iter_mut(world)
    {
        if b.incident == incident && b.leaving.is_none() {
            // Turn aside from the muggers and go.
            let q = glam::Quat::from_array(o.0);
            let f = q * glam::Vec3::Z;
            let away = DVec2::new(f.z as f64, -f.x as f64).normalize_or(DVec2::X);
            b.leaving = Some((away, tick + ticks(LEAVE_SECONDS)));
        }
    }
}

/// Bystanders walking off, and gone once out of sight.
fn walk_bystanders(world: &mut World, tick: u64) {
    let solids = world.resource::<Solids>().0.clone();
    let mut gone = Vec::new();
    for (e, b, mut m, mut o) in world
        .query::<(Entity, &Bystander, &mut Motion, &mut crate::Orientation)>()
        .iter_mut(world)
    {
        let Some((dir, until)) = b.leaving else {
            continue;
        };
        if tick >= until {
            gone.push(e);
            continue;
        }
        let step = dir * LEAVE_SPEED * DT;
        let mut body = m.0;
        let next = DVec2::new(body.pos.x + step.x, body.pos.z + step.y);
        let y = solids.ground_at(next).max(GROUND_Y);
        if (y - body.pos.y).abs() < 0.6 {
            body.pos = DVec3::new(next.x, y, next.y);
            body.vel = DVec3::new(dir.x, 0.0, dir.y) * LEAVE_SPEED;
        } else {
            body.vel = DVec3::ZERO;
        }
        m.0 = body;
        let q = yaw_quat(dir);
        if o.0 != q {
            o.0 = q;
        }
    }
    for e in gone {
        world.despawn(e);
    }
}

/// The town's incidents.
#[derive(Default)]
pub struct Incidents {
    town: Option<Arc<Area>>,
    /// Open ones.
    pub open: Vec<Incident>,
    /// How many have started (seeds the next).
    pub count: u32,
    last_start: Option<u64>,
    /// Seconds before the first one (default [`FIRST_SECONDS`]).
    pub first_after: f64,
    /// The kind the next one will be, if set (tests and screenshots).
    pub next_kind: Option<IncidentKind>,
    /// Screenshots: the next rescue starts with everyone out and the hurt
    /// one in the arms of the hero it started near, who stands beside it.
    pub carried_start: bool,
    /// Start one of this kind at the next second, however many are open
    /// and whenever the last one started (the debug menu).
    pub forced: Option<IncidentKind>,
    /// Spots of incidents that ended, free to use again.
    free: Vec<usize>,
    /// When the colossus last came out.
    last_colossus: Option<u64>,
    /// Buildings that came down since the last second (people trapped).
    collapses: Vec<u64>,
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
    pub fn new(town: Arc<Area>) -> Self {
        Self {
            town: Some(town),
            first_after: FIRST_SECONDS,
            ..Default::default()
        }
    }

    /// Every tick: helping people out. Once a second: start, answer,
    /// resolve and time out incidents.
    pub(crate) fn step(&mut self, world: &mut World, tick: u64) {
        if self.town.is_none() {
            return;
        }
        self.rescue(world);
        walk_bystanders(world, tick);
        if !tick.is_multiple_of(TICK_HZ as u64) {
            return;
        }
        let mut heroes: Vec<(NetId, DVec3, u8)> = world
            .query::<(&Hero, &Net, &Motion, &Fighter)>()
            .iter(world)
            .map(|(_, n, m, f)| (n.0, m.0.pos, f.level))
            .collect();
        heroes.sort_by_key(|h| h.0);
        self.settle(world, tick, &heroes);
        for b in std::mem::take(&mut self.collapses) {
            self.start_collapse(world, tick, b);
        }
        if let Some(kind) = self.forced.take() {
            // A few tries: a place can be taken by another incident.
            let open = self.open.len();
            for _ in 0..8 {
                if heroes.is_empty() || self.open.len() > open {
                    break;
                }
                self.next_kind = Some(kind);
                self.start(world, tick, &heroes);
            }
            self.next_kind = None;
            return;
        }
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

    /// Building `b` (a collider of the city's solids) started to come
    /// down: people are trapped in it.
    pub fn collapsed(&mut self, b: u64) {
        if self.town.is_some() && !self.collapses.contains(&b) {
            self.collapses.push(b);
        }
    }

    /// A rescue in the rubble of building `b`, at its front.
    fn start_collapse(&mut self, world: &mut World, tick: u64, b: u64) {
        let solids = world.resource::<Solids>().0.clone();
        let Some((plan, _)) = solids.building(b) else {
            return;
        };
        let s = &plan.site;
        let p = s.origin + s.along * s.size.x * 0.5 - s.inward * 3.0;
        // One rescue per pile of rubble.
        if self
            .open
            .iter()
            .any(|i| DVec2::new(i.at.x, i.at.z).distance(p) < 30.0)
        {
            return;
        }
        self.count += 1;
        let y = solids.ground_at(p).max(GROUND_Y);
        let inc = self.rescue_at(
            world,
            tick,
            IncidentKind::Collapse,
            DVec3::new(p.x, y, p.y),
            (-s.inward).normalize_or(DVec2::X),
        );
        self.news.push(inc.news());
        self.news.extend(inc.casualty(false));
        self.open.push(inc);
    }

    /// A new rescue of `kind` at `at`, facing out of its place; the
    /// ambulance waits on a straight street near.
    fn rescue_at(
        &self,
        world: &mut World,
        tick: u64,
        kind: IncidentKind,
        at: DVec3,
        facing: DVec2,
    ) -> Incident {
        let town = self.town.as_ref().expect("a town");
        let level = world.resource::<Fights>().team_level;
        let medic = medic_spot(town, &world.resource::<Solids>().0, DVec2::new(at.x, at.z));
        Incident {
            id: self.count,
            kind,
            at,
            spot: None,
            facing,
            left: kind.trapped(),
            freeing: 0.0,
            hurt: None,
            medic: Some(medic),
            level,
            started: tick,
            expires: tick + ticks(kind.timeout()),
            answered: false,
            state: State::Open,
        }
    }

    /// Heroes holding the help key next to the next trapped person get
    /// them out, one after another; the last one out is carried to the
    /// medic.
    fn rescue(&mut self, world: &mut World) {
        if self.open.iter().all(|i| i.medic.is_none()) {
            return;
        }
        let mut helpers: Vec<(NetId, DVec3, u8, bool, bool)> = world
            .query::<(&Hero, &Net, &Motion, &Fighter, &Control)>()
            .iter(world)
            .map(|(_, n, m, f, c)| {
                let up = f.standing();
                (n.0, m.0.pos, f.level, c.0.revive && up, up)
            })
            .collect();
        helpers.sort_by_key(|h| h.0);
        let mut rewards = Vec::new();
        let guardian = |rewards: &mut Vec<combat::Reward>, inc: &Incident, near: DVec3, x: f32| {
            let base = combat::enemy_spec(EnemyKind::Brute).karma;
            for h in helpers.iter().filter(|h| h.1.distance(near) < REWARD) {
                let one = progress::defeat_karma(base, inc.level, h.2) as f32;
                rewards.push(combat::Reward {
                    hero: h.0,
                    karma: (one * inc.kind.bonus() * x).round() as u32,
                    deed: Some(Affinity::Guardian),
                    lean: 0,
                });
            }
        };
        for inc in self.open.iter_mut().filter(|i| i.medic.is_some()) {
            let (medic, _) = inc.medic.expect("a rescue");
            match inc.hurt {
                Some(Hurt::Carried(id)) => {
                    let carrier = helpers.iter().find(|h| h.0 == id);
                    match carrier {
                        Some(h) if h.4 => {
                            let flat = DVec2::new(h.1.x - medic.x, h.1.z - medic.z).length();
                            if flat < MEDIC_REACH && (h.1.y - medic.y).abs() < 4.0 {
                                // Handed over: the rescue is done.
                                let news = inc.casualty(true);
                                inc.hurt = None;
                                guardian(&mut rewards, inc, medic, 2.0);
                                self.news.extend(news);
                            }
                        }
                        // Knocked down (or gone): they drop them.
                        _ => {
                            let at = carrier.map_or(inc.at, |h| h.1);
                            inc.hurt = Some(Hurt::Lying(at));
                            inc.freeing = 0.0;
                            self.news.extend(inc.casualty(false));
                        }
                    }
                    continue;
                }
                Some(Hurt::Lying(at)) => {
                    let lifter = helpers
                        .iter()
                        .find(|h| h.3 && h.1.distance(at) < RESCUE_REACH);
                    match lifter {
                        None => inc.freeing = (inc.freeing - DT).max(0.0),
                        Some(h) => {
                            inc.freeing += DT;
                            if inc.freeing + 1e-9 >= LIFT_SECONDS {
                                inc.freeing = 0.0;
                                inc.hurt = Some(Hurt::Carried(h.0));
                                self.news.extend(inc.casualty(false));
                            }
                        }
                    }
                    continue;
                }
                None if inc.left == 0 => continue,
                None => {}
            }
            let next = *inc.victims().last().expect("someone trapped");
            let helper = helpers
                .iter()
                .filter(|h| h.3 && h.1.distance(next) < RESCUE_REACH)
                .min_by(|a, b| a.1.distance(next).total_cmp(&b.1.distance(next)));
            let Some(helper) = helper else {
                inc.freeing = (inc.freeing - DT).max(0.0);
                continue;
            };
            inc.answered = true;
            inc.freeing += DT;
            if inc.freeing + 1e-9 < inc.kind.rescue_seconds() {
                continue;
            }
            inc.freeing = 0.0;
            inc.left -= 1;
            // Karma for everyone who helped or stood by.
            guardian(&mut rewards, inc, next, 1.0);
            self.news.push(inc.news());
            if inc.left == 0 {
                // The last one out is hurt: in the arms of who freed them.
                inc.hurt = Some(Hurt::Carried(helper.0));
                self.news.extend(inc.casualty(false));
            }
        }
        world.resource_mut::<Fights>().rewards.extend(rewards);
    }

    /// Answered, resolved or timed out.
    fn settle(&mut self, world: &mut World, tick: u64, heroes: &[(NetId, DVec3, u8)]) {
        // The walking tank moves on: its call follows it.
        let tanks: Vec<(usize, DVec3)> = world
            .query::<(&combat::Enemy, &Motion)>()
            .iter(world)
            .filter(|(e, _)| e.kind == EnemyKind::Colossus)
            .map(|(e, m)| (e.spot, m.0.pos))
            .collect();
        for inc in self.open.iter_mut() {
            let Some(at) = inc
                .spot
                .and_then(|s| tanks.iter().find(|t| t.0 == s))
                .map(|t| t.1)
            else {
                continue;
            };
            if at.distance(inc.at) > 20.0 {
                inc.at = at;
                self.news.push(inc.news());
            }
        }
        let mut ended = Vec::new();
        for (k, inc) in self.open.iter_mut().enumerate() {
            if heroes.iter().any(|h| h.1.distance(inc.at) < ANSWER) {
                inc.answered = true;
            }
            let beaten = match inc.spot {
                Some(i) => {
                    let spot = &world.resource::<Fights>().spots[i];
                    spot.groups > 0 && spot.members == 0
                }
                None => inc.left == 0 && inc.hurt.is_none(),
            };
            if beaten {
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
            release_bystanders(world, tick, inc.id);
            if let Some(spot) = inc.spot {
                match inc.state {
                    State::Resolved => self.reward(world, &inc, heroes),
                    _ => Self::send_home(world, spot),
                }
                self.free.push(spot);
            }
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
        let homes: Vec<_> = fronts(BuildingUse::Apartments)
            .into_iter()
            .chain(fronts(BuildingUse::House))
            .chain(fronts(BuildingUse::Tenement))
            .collect();
        let streets: Vec<(DVec2, DVec2)> = town
            .roads
            .iter()
            .filter(|r| !r.main)
            .flat_map(|r| r.pts.windows(2).map(|w| (w[0], (w[1] - w[0]).perp())))
            .filter(|(p, _)| ring(*p) && !held(*p))
            .collect();
        let mut kinds: Vec<(IncidentKind, f64)> = vec![
            (IncidentKind::Mugging, 0.25),
            (IncidentKind::Robbery, 0.2),
            (IncidentKind::Rampage, 0.15),
            (IncidentKind::Breakout, 0.12),
            (IncidentKind::CarCrash, 0.14),
            (IncidentKind::Fire, 0.14),
            (IncidentKind::Colossus, 0.08),
        ];
        // The walking tank comes out of the Tesla tower now and then, once
        // the team can take it, and never two at once.
        let tower = town
            .landmarks
            .iter()
            .find(|l| l.kind == LandmarkKind::TeslaTower)
            .map(|l| {
                // Out onto the nearest street wide enough for it.
                let front = l.pos + l.dir * (l.plaza + 14.0);
                let street = world
                    .resource::<Fights>()
                    .streets
                    .as_ref()
                    .and_then(|s| s.nearest(front).map(|n| s.node(n)))
                    .filter(|p| p.distance(front) < 120.0);
                match street {
                    Some(p) => (p, (p - l.pos).normalize_or(l.dir)),
                    None => (front, l.dir),
                }
            });
        let team = world.resource::<Fights>().team_level;
        let colossus_ok = tower.is_some()
            && !self.open.iter().any(|i| i.kind == IncidentKind::Colossus)
            && (self.next_kind == Some(IncidentKind::Colossus)
                || (team >= COLOSSUS_LEVEL
                    && self
                        .last_colossus
                        .is_none_or(|t| tick >= t + ticks(COLOSSUS_GAP))));
        kinds.retain(|(k, _)| match k {
            IncidentKind::Breakout => !labs.is_empty(),
            IncidentKind::Robbery => !shops.is_empty(),
            IncidentKind::Fire => !homes.is_empty(),
            IncidentKind::Colossus => colossus_ok,
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
        if let Some(k) = self.next_kind.take() {
            if kinds.iter().any(|x| x.0 == k) {
                kind = k;
            }
        }
        let tower_place: Vec<(DVec2, DVec2)> = tower.into_iter().collect();
        let places = match kind {
            IncidentKind::Breakout => &labs,
            IncidentKind::Robbery => &shops,
            IncidentKind::Fire => &homes,
            IncidentKind::Colossus => &tower_place,
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
        // A fight faces the hero who will come; a rescue the way its place
        // does (across the road, out of the house).
        let facing = if kind.trapped() > 0 {
            face.normalize_or(DVec2::X)
        } else {
            (me - p).normalize_or(face.normalize_or(DVec2::X))
        };
        let (size, only, extra) = kind.group();
        if kind.trapped() > 0 {
            let mut inc = self.rescue_at(world, tick, kind, at, facing);
            if std::mem::take(&mut self.carried_start) {
                let id = heroes
                    .iter()
                    .min_by(|a, b| a.1.distance(at).total_cmp(&b.1.distance(at)))
                    .map(|h| h.0);
                let mut q = world.query::<(&Net, &mut Motion)>();
                for (n, mut m) in q.iter_mut(world) {
                    if Some(n.0) == id {
                        m.0.pos = at + DVec3::new(facing.x, 0.0, facing.y) * 5.0 + DVec3::Y * 0.3;
                        m.0.vel = DVec3::ZERO;
                    }
                }
                if let Some(id) = id {
                    inc.left = 0;
                    inc.answered = true;
                    inc.hurt = Some(Hurt::Carried(id));
                }
            }
            self.news.push(inc.news());
            self.news.extend(inc.casualty(false));
            self.open.push(inc);
            return;
        }
        let spot = Spot {
            extra,
            size: Some(size),
            only,
            once: true,
            lead: kind.lead(),
            provoke: Some(kind.reach()),
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
            spot: Some(index),
            facing,
            left: 0,
            freeing: 0.0,
            hurt: None,
            medic: None,
            level,
            started: tick,
            expires: tick + ticks(kind.timeout()),
            answered: false,
            state: State::Open,
        };
        if kind == IncidentKind::Colossus {
            self.last_colossus = Some(tick);
        }
        if kind == IncidentKind::Mugging {
            spawn_bystander(world, tick, inc.id, mugged(at, facing), -facing);
        }
        self.news.push(inc.news());
        self.open.push(inc);
    }

    /// Every open incident, for a player who joins.
    pub fn snapshot(&self) -> Vec<ToClient> {
        let mut all: Vec<ToClient> = self.open.iter().map(Incident::news).collect();
        all.extend(self.open.iter().filter_map(|i| i.casualty(false)));
        all
    }

    pub fn take_news(&mut self) -> Vec<ToClient> {
        std::mem::take(&mut self.news)
    }
}

/// Where an ambulance waits for a rescue at `p`: parked at the kerb of a
/// street about [`MEDIC_AWAY`] off, along it. Only straight stretches, clear
/// of junctions, so it stands square with the kerb and not on a corner.
fn medic_spot(town: &Area, solids: &crate::collision::Obstacles, p: DVec2) -> (DVec3, DVec2) {
    use hw_gen::water::segment_distance;
    let quarter = town.town.dystopia.as_ref();
    // Every 4 m along every road: where, which way, how far off we'd like.
    let mut spots: Vec<(f64, DVec2, DVec2, usize)> = Vec::new();
    for (ri, r) in town.roads.iter().enumerate() {
        let len: f64 = r.pts.windows(2).map(|w| w[0].distance(w[1])).sum();
        let at = |s: f64| -> Option<(DVec2, DVec2)> {
            let mut s = s;
            for w in r.pts.windows(2) {
                let l = w[0].distance(w[1]);
                if s <= l && l > 1e-6 {
                    let d = (w[1] - w[0]) / l;
                    return Some((w[0] + d * s, d));
                }
                s -= l;
            }
            None
        };
        let mut s = MEDIC_CLEAR;
        while s <= len - MEDIC_CLEAR {
            let here = at(s);
            let (back, ahead) = (at(s - 9.0), at(s + 9.0));
            s += 4.0;
            let (Some((c, d)), Some((_, db)), Some((_, da))) = (here, back, ahead) else {
                continue;
            };
            // Straight for the ambulance's length and a little more.
            if d.dot(db) < 0.995 || d.dot(da) < 0.995 {
                continue;
            }
            let kerb = c + d.perp() * (r.half - 1.6).max(0.0);
            if quarter.is_some_and(|q| q.contains(kerb)) {
                continue;
            }
            spots.push(((kerb.distance(p) - MEDIC_AWAY).abs(), kerb, d, ri));
        }
    }
    spots.sort_by(|a, b| a.0.total_cmp(&b.0));
    // The nearest fit with no other road joining beside it.
    let clear = |kerb: DVec2, own: usize| {
        town.roads.iter().enumerate().all(|(i, r)| {
            i == own
                || r.pts
                    .windows(2)
                    .all(|w| segment_distance(kerb, w[0], w[1]) > r.half + MEDIC_CLEAR * 0.5)
        })
    };
    let pick = spots.iter().find(|s| clear(s.1, s.3)).or(spots.first());
    let (at, along) = pick.map_or((p + DVec2::X * 20.0, DVec2::X), |b| (b.1, b.2));
    let y = solids.ground_at(at).max(GROUND_Y);
    (DVec3::new(at.x, y, at.y), along)
}
