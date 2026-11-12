//! Street crime: the gangs' small evil in plain view. A Chrome Jacket
//! snatches a handbag and runs; two of them block a passer-by's way and
//! shove them about; they lean on a shopkeeper for the till money, or tag
//! a wall with their sign; after dark a Velvet Syndicate gunsel collects
//! an envelope of protection money at a shop door, and the mad scientist's
//! brutes smash a street lamp or kick a bin over near his quarter.
//!
//! Like the street acts it is a pure function of the town seed and the
//! game clock: where it can happen (worked out per patch of town, only
//! near where someone asks, and forgotten again), which crime happens at
//! which place in which hour of the day, and what everyone in it is doing
//! at a given second. Every machine in a co-op game sees the same crimes,
//! and the server knows them too, without anything sent: a hero who comes
//! close while one is going on steps in, and the server makes its crooks
//! real villains where they stand (see `crate::crimes`).
//!
//! Crime keeps to the rough streets: the frayed band round the villains'
//! quarter most, the streets near it a good deal, downtown now and then,
//! never in front of Hero Hall. Everyone walks in along the pavement and
//! away again; nobody appears or vanishes in the middle of the street.

use super::{cell, Cache, StreetLife, PATCH};
use crate::citizens::LOOKS;
use crate::clock;
use crate::combat::EnemyKind;
use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_gen::building::BuildingUse;
use hw_gen::geom::Polyline;
use hw_gen::urban::dystopia::Zone;
use hw_gen::urban::landmarks::LandmarkKind;
use hw_gen::urban::props::PropKind;
use std::sync::Arc;

/// Game seconds a crime place keeps one crime at most: an hour of the day.
pub const SLOT: f64 = clock::SECONDS_PER_HOUR;
/// A tag stays on its wall this long after it is sprayed (s) — and longer
/// while someone is near enough to see it go (whoever draws it keeps it
/// till then): it is cleaned off overnight.
pub const TAG_LIFE: f64 = 8.0 * clock::SECONDS_PER_HOUR;
/// How many patches' crime places, and how many crimes, are kept at once.
const KEEP_PATCHES: usize = 160;
const KEEP_CRIMES: usize = 512;
/// Two crime places at least this far apart (m), and this far from a
/// street act's pitch.
const SPACING: f64 = 14.0;
const PITCH_CLEAR: f64 = 10.0;
/// How far crooks and victims walk in and out along the pavement (m): at
/// most, and at least (a place with less room either way is no place).
const WAY: f64 = 38.0;
const WAY_MIN: f64 = 16.0;
/// Paces (m/s): walking, a passer-by, a hurried passer-by, a thief running.
const STROLL: f64 = 1.25;
const PASSER: f64 = 1.35;
const HURRY: f64 = 3.0;
const RUN: f64 = 5.2;

/// What kind of crime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CrimeKind {
    /// A greaser snatches a handbag and runs (Chrome Jackets).
    Snatch,
    /// Two greasers block a passer-by's way and push them about (Jackets).
    Hassle,
    /// Two greasers lean on a shopkeeper for the till money (Jackets).
    Shakedown,
    /// Two Velvet Syndicate gunsels collect protection money at a shop
    /// door, after dark.
    Protection,
    /// A greaser sprays the gang's sign on a wall while another keeps
    /// watch (Jackets).
    Tagging,
    /// Two of the mad scientist's brutes smash a street lamp or kick over a
    /// bin, near his quarter.
    Vandals,
}

impl CrimeKind {
    pub const ALL: [CrimeKind; 6] = [
        CrimeKind::Snatch,
        CrimeKind::Hassle,
        CrimeKind::Shakedown,
        CrimeKind::Protection,
        CrimeKind::Tagging,
        CrimeKind::Vandals,
    ];

    /// What people call it.
    pub fn title(self) -> &'static str {
        match self {
            CrimeKind::Snatch => "Bag snatch",
            CrimeKind::Hassle => "Greasers hassling someone",
            CrimeKind::Shakedown => "Shakedown",
            CrimeKind::Protection => "Protection money",
            CrimeKind::Tagging => "Gang tagging a wall",
            CrimeKind::Vandals => "Brutes smashing the street",
        }
    }

    /// Who does it.
    pub fn crook(self) -> EnemyKind {
        match self {
            CrimeKind::Protection => EnemyKind::Gunsel,
            CrimeKind::Vandals => EnemyKind::Brute,
            _ => EnemyKind::Greaser,
        }
    }

    /// How many of them.
    pub fn crooks(self) -> u8 {
        match self {
            CrimeKind::Snatch => 1,
            _ => 2,
        }
    }

    /// The kind of place it needs.
    pub fn place(self) -> PlaceKind {
        match self {
            CrimeKind::Snatch | CrimeKind::Hassle => PlaceKind::Pavement,
            CrimeKind::Shakedown | CrimeKind::Protection => PlaceKind::Door,
            CrimeKind::Tagging => PlaceKind::Wall,
            CrimeKind::Vandals => PlaceKind::Prop,
        }
    }

    /// Hours of the day it happens, [from, to) (to may run past midnight).
    fn hours(self) -> (f64, f64) {
        match self {
            CrimeKind::Snatch => (8.0, 23.0),
            CrimeKind::Hassle => (11.0, 26.0),
            CrimeKind::Shakedown => (8.5, 19.0),
            CrimeKind::Protection => (19.0, 28.0),
            CrimeKind::Tagging => (16.0, 29.0),
            CrimeKind::Vandals => (17.0, 29.5),
        }
    }

    /// Does it happen at `hour`?
    pub fn on_at(self, hour: f64) -> bool {
        let h = hour.rem_euclid(24.0);
        let (a, b) = self.hours();
        (h >= a && h < b) || (h + 24.0 >= a && h + 24.0 < b)
    }

    /// How rough the street must be for it (see [`StreetLife::rough_at`]).
    fn needs(self) -> f64 {
        match self {
            CrimeKind::Vandals => 0.55,
            CrimeKind::Tagging => 0.3,
            CrimeKind::Hassle => 0.2,
            _ => 0.0,
        }
    }

    /// How likely it is where it fits, against the others.
    fn weight(self, rough: f64) -> f64 {
        match self {
            CrimeKind::Snatch => 1.0,
            CrimeKind::Hassle => 0.9,
            CrimeKind::Shakedown => 1.0,
            // The Syndicate works the respectable streets: less where the
            // Jackets are.
            CrimeKind::Protection => 1.4 - rough * 0.8,
            CrimeKind::Tagging => 1.0,
            CrimeKind::Vandals => 1.0,
        }
    }
}

/// What a crime place is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PlaceKind {
    /// A point on the line people walk along a pavement.
    Pavement,
    /// A shop's street door.
    Door,
    /// A stretch of blank wall along a pavement.
    Wall,
    /// A breakable thing in the street: a lamp, a bin, a mailbox.
    Prop,
}

/// A place a crime can happen.
#[derive(Clone, Debug, PartialEq)]
pub struct CrimePlace {
    /// Stable for the town: the same place every time it is worked out.
    pub key: u64,
    pub kind: PlaceKind,
    /// The middle of it: where a victim is stopped, a shopkeeper stands,
    /// the wall is sprayed, the thing stands.
    pub at: DVec2,
    /// Height of the ground there.
    pub y: f64,
    /// Along the pavement (unit), and from the pavement towards the houses.
    pub along: DVec2,
    pub inner: DVec2,
    /// The places people stand, by the kind of place (see
    /// [`StreetLife::plan_crime`]).
    pub spots: Vec<DVec2>,
    /// A shop's door, in its facade.
    pub door: Option<DVec2>,
    /// A thing to smash: its id in the town and the kind.
    pub prop: Option<(u64, PropKind)>,
    /// The way along the pavement, from where the place meets it, in
    /// either direction (+along, -along).
    ways: [Arc<Vec<DVec2>>; 2],
    /// How rough the street is (0 to 1).
    pub rough: f64,
}

/// Who someone in a crime is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CrimeRole {
    /// One of the crooks: their kind, a number picking their looks (see
    /// whoever draws them) and which of them.
    Crook(EnemyKind, u8, u8),
    /// The victim: one of the citizens' looks.
    Victim(u32),
}

/// What someone in a crime is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Deed {
    Walking,
    /// Hurrying off (a victim glad to get away).
    Hurrying,
    /// Running off with the loot.
    Running,
    /// Lounging against a wall, waiting.
    Lounging,
    /// Keeping watch: looking up and down the street.
    Watching,
    /// Leaning in: a fist shaken, a finger jabbed.
    Threatening,
    /// A shove in the chest.
    Shoving,
    /// Snatching something out of someone's hands.
    Grabbing,
    /// Laughing it up.
    Laughing,
    /// Counting the money.
    Counting,
    /// Spraying a wall.
    Spraying,
    /// Swinging at a thing to smash it.
    Smashing,
    /// Hands up, shrinking back.
    Cowering,
    /// Shouting after a thief.
    Shouting,
    /// Handing something over.
    HandingOver,
    /// Standing shaken.
    Shaken,
}

/// Something carried in a crime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Loot {
    Handbag,
    /// A fold of banknotes from the till.
    Cash,
    /// A fat envelope of protection money.
    Envelope,
    SprayCan,
}

/// Someone of a crime at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrimePerson {
    /// Stable while they are about.
    pub id: u64,
    pub role: CrimeRole,
    /// Feet on the ground.
    pub pos: DVec3,
    /// The way they face (unit, in the ground plane).
    pub facing: DVec2,
    /// Moving speed now (0 standing).
    pub speed: f64,
    pub deed: Deed,
    /// What they hold in their right hand.
    pub holding: Option<Loot>,
}

/// A gang's sign sprayed on a wall.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tag {
    /// Stable: the crime's key.
    pub key: (u64, i64),
    /// Its middle, on the wall's face.
    pub at: DVec3,
    /// Out of the wall (unit, in the ground plane).
    pub normal: DVec2,
    /// Its width (m), and which picture.
    pub size: f64,
    pub image: u8,
    /// Spraying starts and ends.
    pub from: f64,
    pub to: f64,
}

impl Tag {
    /// How much of it is there at `t` (0 to 1).
    pub fn done(&self, t: f64) -> f64 {
        ((t - self.from) / (self.to - self.from).max(1e-3)).clamp(0.0, 1.0)
    }

    /// Is it on the wall at `t` (sprayed, and not cleaned off yet)?
    pub fn on(&self, t: f64) -> bool {
        t >= self.from && t < self.to + TAG_LIFE
    }
}

/// A stretch of someone's part: standing somewhere doing something, or
/// moving along a path.
#[derive(Clone, Debug)]
enum Beat {
    Stand {
        at: DVec2,
        face: DVec2,
        deed: Deed,
    },
    Move {
        path: Arc<Polyline>,
        from: f64,
        to: f64,
        deed: Deed,
    },
}

#[derive(Clone, Debug)]
struct Part {
    id: u64,
    role: CrimeRole,
    /// (from, to, what), in order, back to back.
    beats: Vec<(f64, f64, Beat)>,
    /// What they hold, and when.
    holds: Vec<(f64, f64, Loot)>,
}

impl Part {
    fn new(id: u64, role: CrimeRole) -> Self {
        Self {
            id,
            role,
            beats: Vec::new(),
            holds: Vec::new(),
        }
    }

    /// The end of the last beat (or `t` if none yet).
    fn end(&self, t: f64) -> f64 {
        self.beats.last().map_or(t, |b| b.1)
    }

    fn stand(&mut self, t0: f64, t1: f64, at: DVec2, face: DVec2, deed: Deed) -> &mut Self {
        if t1 > t0 {
            self.beats.push((
                t0,
                t1,
                Beat::Stand {
                    at,
                    face: face.normalize_or(DVec2::Y),
                    deed,
                },
            ));
        }
        self
    }

    /// Walks `path` backwards (from its far end in to its start), arriving
    /// at `arrive`.
    fn come(&mut self, path: &Arc<Polyline>, pace: f64, arrive: f64) -> &mut Self {
        let len = path.len();
        self.beats.push((
            arrive - len / pace,
            arrive,
            Beat::Move {
                path: path.clone(),
                from: len,
                to: 0.0,
                deed: Deed::Walking,
            },
        ));
        self
    }

    /// Goes off along `path` from its start at `t`, at `pace`.
    fn go(&mut self, path: &Arc<Polyline>, pace: f64, t: f64, deed: Deed) -> &mut Self {
        let len = path.len();
        self.beats.push((
            t,
            t + len / pace,
            Beat::Move {
                path: path.clone(),
                from: 0.0,
                to: len,
                deed,
            },
        ));
        self
    }

    /// A few steps from `a` to `b` in `secs`.
    fn step(&mut self, t: f64, secs: f64, a: DVec2, b: DVec2, deed: Deed) -> &mut Self {
        let path = Arc::new(Polyline::new(&[a, b]));
        self.beats.push((
            t,
            t + secs,
            Beat::Move {
                path,
                from: 0.0,
                to: a.distance(b),
                deed,
            },
        ));
        self
    }

    fn hold(&mut self, t0: f64, t1: f64, loot: Loot) -> &mut Self {
        self.holds.push((t0, t1, loot));
        self
    }

    fn first(&self) -> f64 {
        self.beats.first().map_or(f64::INFINITY, |b| b.0)
    }

    fn at(&self, life: &StreetLife, t: f64) -> Option<CrimePerson> {
        let (t0, t1, beat) = self.beats.iter().find(|b| t >= b.0 && t < b.1)?;
        let (p, facing, speed, deed) = match beat {
            Beat::Stand { at, face, deed } => (*at, *face, 0.0, *deed),
            Beat::Move {
                path,
                from,
                to,
                deed,
            } => {
                let k = ((t - t0) / (t1 - t0).max(1e-6)).clamp(0.0, 1.0);
                let (q, d) = path.sample(from + (to - from) * k);
                let dir = if to >= from { d } else { -d };
                let speed = (to - from).abs() / (t1 - t0).max(1e-6);
                (q, dir, speed, *deed)
            }
        };
        let holding = self.holds.iter().find(|h| t >= h.0 && t < h.1).map(|h| h.2);
        Some(CrimePerson {
            id: self.id,
            role: self.role,
            pos: DVec3::new(p.x, life.ground_at(p), p.y),
            facing,
            speed,
            deed,
            holding,
        })
    }
}

/// One crime at one place in one slot.
#[derive(Clone, Debug)]
pub struct Crime {
    pub place: Arc<CrimePlace>,
    pub kind: CrimeKind,
    pub slot: i64,
    seed: Seed,
    parts: Vec<Part>,
    /// The crooks are there and at it from `on` (and stepping in counts)
    /// until the last of them is gone.
    pub on: f64,
    /// The crime itself is done (the bag snatched, the money handed over,
    /// the tag finished, the lamp smashed); they make off after.
    pub done: f64,
    /// When the first of them comes, and the last is gone.
    pub first: f64,
    pub gone: f64,
    /// A thing smashed: when.
    pub smash: Option<f64>,
    /// A sign sprayed.
    pub tag: Option<Tag>,
}

impl Crime {
    /// Its key: its place and slot.
    pub fn key(&self) -> (u64, i64) {
        (self.place.key, self.slot)
    }

    /// A number that varies between crimes (lines, little things).
    pub fn seed(&self) -> u64 {
        self.seed.0
    }

    /// Is anyone of it about at `t`?
    pub fn active(&self, t: f64) -> bool {
        self.first <= t && t < self.gone
    }

    /// Is it going on at `t` (the crooks at it, or making off)? A hero who
    /// comes close then steps in.
    pub fn hot(&self, t: f64) -> bool {
        self.on <= t && t < self.gone
    }

    /// Everyone of it at `t`.
    pub fn people(&self, life: &StreetLife, t: f64) -> Vec<CrimePerson> {
        self.parts.iter().filter_map(|p| p.at(life, t)).collect()
    }

    /// The crooks at `t`.
    pub fn crooks(&self, life: &StreetLife, t: f64) -> Vec<CrimePerson> {
        self.people(life, t)
            .into_iter()
            .filter(|p| matches!(p.role, CrimeRole::Crook(..)))
            .collect()
    }

    /// The victim at `t`, if about.
    pub fn victim(&self, life: &StreetLife, t: f64) -> Option<CrimePerson> {
        self.people(life, t)
            .into_iter()
            .find(|p| matches!(p.role, CrimeRole::Victim(_)))
    }

    /// The crooks' kind.
    pub fn crook(&self) -> EnemyKind {
        self.kind.crook()
    }
}

/// How the town has it at a place: the gangs' streets, the rest of town.
fn rough_of(life: &StreetLife, p: DVec2) -> f64 {
    let a = &life.area;
    // Nobody tries it in front of the town's heroes.
    if a.landmarks
        .iter()
        .any(|l| l.kind == LandmarkKind::HeroHall && l.pos.distance(p) < l.plaza + 120.0)
    {
        return 0.0;
    }
    let base = |p: DVec2| {
        let old = a
            .town
            .pieces
            .iter()
            .find(|pc| hw_gen::urban::contains(&pc.face.pts, p))
            .is_some_and(|pc| pc.era == hw_gen::urban::growth::Era::Old);
        if old {
            0.16
        } else {
            0.1
        }
    };
    match &a.town.dystopia {
        Some(d) => match d.zone(p) {
            // Its own corners are the quarter's business (see `quarter`).
            Zone::Dystopia => 0.0,
            Zone::Edge => 1.0,
            _ => {
                let far = d.distance(p);
                if far < 320.0 {
                    (1.0 - (far - hw_gen::urban::dystopia::BAND) / 250.0 * 0.75).clamp(base(p), 1.0)
                } else {
                    base(p)
                }
            }
        },
        None => base(p),
    }
}

/// Chance a place sees a crime in an hour, by its kind, on the roughest
/// street.
fn rate(kind: PlaceKind) -> f64 {
    match kind {
        PlaceKind::Pavement => 0.24,
        PlaceKind::Door => 0.26,
        PlaceKind::Wall => 0.2,
        PlaceKind::Prop => 0.22,
    }
}

/// The crime places and crimes of one town square.
pub(super) struct CrimeCache {
    patches: Cache<(i64, i64), Arc<Vec<Arc<CrimePlace>>>>,
    crimes: Cache<(u64, i64), Option<Arc<Crime>>>,
}

impl CrimeCache {
    pub(super) fn new() -> Self {
        Self {
            patches: Cache::new(KEEP_PATCHES),
            crimes: Cache::new(KEEP_CRIMES),
        }
    }

    pub(super) fn held(&self) -> (usize, usize) {
        (self.patches.len(), self.crimes.len())
    }
}

impl StreetLife {
    /// How rough the street is at `p` (0 to 1): crime is likelier the
    /// rougher it is.
    pub fn rough_at(&self, p: DVec2) -> f64 {
        rough_of(self, p)
    }

    /// Can someone stand at `p`: out of the houses, clear of everything
    /// standing in the street, on dry ground?
    fn stand_ok(&self, p: DVec2) -> bool {
        let plinth = self
            .area
            .landmarks
            .iter()
            .any(|l| l.pos.distance(p) <= l.plaza);
        (plinth || self.area.ground(p) >= hw_gen::urban::world::DECK + 0.2)
            && !self.in_building(p, 0.25)
            && self.prop_distance(p, 1.0) >= 0.4
    }

    /// The way along road `ri`'s pavement (at its walking line, on side
    /// `side`), from distance `s` in direction `dir` (+1/-1), as far as it
    /// is clear, at most [`WAY`] and never past the street's end. `None`
    /// if there is not [`WAY_MIN`] of it, or it ends less than three
    /// quarters of that from where it starts.
    fn way(&self, ri: u32, side: f64, s: f64, dir: f64) -> Option<Arc<Vec<DVec2>>> {
        let road = &self.area.roads[ri as usize];
        let line = &self.index().lines[ri as usize];
        let len = line.len();
        let mut pts = Vec::new();
        let mut d = 0.0;
        while d <= WAY {
            let x = s + dir * d;
            if !(2.0..=len - 2.0).contains(&x) {
                break;
            }
            let (q, along) = line.sample(x);
            let p = q + along.perp() * side * road.walk_offset();
            if d > 0.5 && !self.stand_ok(p) {
                break;
            }
            pts.push(p);
            d += 1.0;
        }
        // (Round a sharp bend the way's far end can lie close by: people
        // must come from a way off, not from round the corner.)
        let far = match (pts.first(), pts.last()) {
            (Some(a), Some(b)) => a.distance(*b) >= WAY_MIN * 0.75,
            _ => false,
        };
        (pts.len() as f64 > WAY_MIN && far).then(|| Arc::new(pts))
    }

    /// The crime places in patch `key`.
    pub fn crime_places(&self, key: (i64, i64)) -> Arc<Vec<Arc<CrimePlace>>> {
        if let Some(p) = self.crime.lock().expect("crime").patches.get(&key) {
            return p;
        }
        let places = Arc::new(self.plan_places(key));
        self.crime
            .lock()
            .expect("crime")
            .patches
            .put(key, places.clone());
        places
    }

    fn plan_places(&self, key: (i64, i64)) -> Vec<Arc<CrimePlace>> {
        let lo = DVec2::new(key.0 as f64, key.1 as f64) * PATCH;
        let hi = lo + DVec2::splat(PATCH);
        let inside = |p: DVec2| {
            p.x >= lo.x + 3.0 && p.y >= lo.y + 3.0 && p.x < hi.x - 3.0 && p.y < hi.y - 3.0
        };
        let seed = self.seed.child("crime places").cell(key.0, key.1);
        let ix = self.index();
        // The street acts' pitches here: crime keeps clear of them.
        let pitches: Vec<DVec2> = self.patch(key).iter().map(|s| s.at).collect();
        let mut out: Vec<CrimePlace> = Vec::new();
        let taken = |p: DVec2, out: &Vec<CrimePlace>| {
            out.iter().any(|c| c.at.distance(p) < SPACING)
                || pitches.iter().any(|q| q.distance(p) < PITCH_CLEAR)
        };
        let (c0, c1) = (cell(lo, super::CELL), cell(hi, super::CELL));
        let gather = |map: &std::collections::BTreeMap<(i64, i64), Vec<u32>>| {
            let mut list: Vec<u32> = Vec::new();
            for x in c0.0..=c1.0 {
                for z in c0.1..=c1.1 {
                    if let Some(l) = map.get(&(x, z)) {
                        list.extend(l);
                    }
                }
            }
            list.sort_unstable();
            list.dedup();
            list
        };
        // Where a point by the houses meets a pavement: the road, its side,
        // how far along, and the walking line's point.
        let meet = |p: DVec2| -> Option<(u32, f64, f64, DVec2, DVec2)> {
            let (ri, kerb) = self.road_in_front(p)?;
            let road = &self.area.roads[ri as usize];
            if road.pavement < 1.4 || kerb > road.pavement + 1.2 {
                return None;
            }
            let line = &ix.lines[ri as usize];
            let (s, _) = line.nearest(p);
            let (q, along) = line.sample(s);
            let side = if (p - q).dot(along.perp()) >= 0.0 {
                1.0
            } else {
                -1.0
            };
            let w = q + along.perp() * side * road.walk_offset();
            Some((ri, side, s, w, along))
        };
        let ways = |ri: u32, side: f64, s: f64| -> Option<[Arc<Vec<DVec2>>; 2]> {
            Some([self.way(ri, side, s, 1.0)?, self.way(ri, side, s, -1.0)?])
        };
        // Shop doors (shakedowns, protection) and blank walls (tags).
        for b in gather(&ix.buildings) {
            let s = &self.area.buildings[b as usize].site;
            let mid = s.origin + s.along * s.size.x * 0.5;
            if !inside(mid) || s.size.x < 6.0 {
                continue;
            }
            let rough = rough_of(self, mid);
            if rough <= 0.0 {
                continue;
            }
            let mut rng = seed.child(b as u64).rng();
            let out_dir = -s.inward;
            let shop = s.usage == BuildingUse::ShopsAndOffices;
            let doors = if shop || rng.chance(0.5) {
                self.doors_of(b)
            } else {
                Vec::new()
            };
            if shop && rng.chance(0.55) {
                let door = doors
                    .iter()
                    .copied()
                    .filter(|d| (d - s.origin).dot(s.inward).abs() < 0.6)
                    .min_by(|a, c| a.distance(mid).total_cmp(&c.distance(mid)));
                if let Some(door) = door {
                    let owner = door + out_dir * 0.75;
                    let c1 = owner + out_dir * 1.2;
                    let c2 = owner + out_dir * 1.05 + s.along * 1.05;
                    let fit = [owner, c1, c2].iter().all(|p| self.stand_ok(*p));
                    if fit && !taken(owner, &out) {
                        if let Some((ri, side, sw, _, along)) = meet(owner) {
                            if let Some(w) = ways(ri, side, sw) {
                                out.push(CrimePlace {
                                    key: 0,
                                    kind: PlaceKind::Door,
                                    at: owner,
                                    y: self.ground_at(owner),
                                    along,
                                    inner: s.inward,
                                    spots: vec![owner, c1, c2],
                                    door: Some(door),
                                    prop: None,
                                    ways: w,
                                    rough,
                                });
                            }
                        }
                    }
                }
            }
            // A blank stretch of the front for a tag.
            if rough >= CrimeKind::Tagging.needs() && rng.chance(0.4) {
                let x = rng.range(1.5, (s.size.x - 1.5).max(1.6));
                let wall = s.origin + s.along * x;
                let clear = doors.iter().all(|d| d.distance(wall) > 2.6);
                let tagger = wall + out_dir * 0.6;
                let lookout = wall + out_dir * 1.9 + s.along * 1.3;
                if clear && self.stand_ok(tagger) && self.stand_ok(lookout) && !taken(tagger, &out)
                {
                    if let Some((ri, side, sw, _, along)) = meet(tagger) {
                        if let Some(w) = ways(ri, side, sw) {
                            out.push(CrimePlace {
                                key: 0,
                                kind: PlaceKind::Wall,
                                at: wall,
                                y: self.ground_at(tagger),
                                along,
                                inner: s.inward,
                                spots: vec![tagger, lookout],
                                door: None,
                                prop: None,
                                ways: w,
                                rough,
                            });
                        }
                    }
                }
            }
        }
        // Points along pavements (snatches, hassling).
        for ri in gather(&ix.roads) {
            let road = &self.area.roads[ri as usize];
            if road.pavement < 1.4 || road.pts.len() < 2 {
                continue;
            }
            let line = &ix.lines[ri as usize];
            let len = line.len();
            let mut rng = seed.child("pavement").child(road.id.0).rng();
            let mut d = rng.range(WAY_MIN + 2.0, WAY_MIN + 22.0);
            while d < len - WAY_MIN - 2.0 {
                let (q, along) = line.sample(d);
                let here = d;
                d += rng.range(26.0, 46.0);
                let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
                let at = q + along.perp() * side * road.walk_offset();
                if !inside(at) || taken(at, &out) {
                    continue;
                }
                let rough = rough_of(self, at);
                if rough <= 0.0 || !self.stand_ok(at) {
                    continue;
                }
                // From the walking line towards the houses.
                let inner = along.perp() * side;
                let room = (road.pavement * 0.45).clamp(0.4, 0.9);
                let lean = |k: f64| at + inner * room + along * (0.7 * k);
                let spots = vec![at, lean(1.0), lean(-1.0)];
                if !spots.iter().all(|p| self.stand_ok(*p)) {
                    continue;
                }
                let Some(w) = ways(ri, side, here) else {
                    continue;
                };
                out.push(CrimePlace {
                    key: 0,
                    kind: PlaceKind::Pavement,
                    at,
                    y: self.ground_at(at),
                    along,
                    inner,
                    spots,
                    door: None,
                    prop: None,
                    ways: w,
                    rough,
                });
            }
        }
        // Lamps, bins and the like near the quarter.
        for i in gather(&ix.props) {
            let p = &self.area.props[i as usize];
            if !inside(p.pos)
                || !matches!(
                    p.kind,
                    PropKind::Lamp(_)
                        | PropKind::Bin
                        | PropKind::Mailbox
                        | PropKind::Hydrant
                        | PropKind::PhoneBooth
                )
                || crate::props::toughness(p.kind).is_none()
            {
                continue;
            }
            let rough = rough_of(self, p.pos);
            if rough < CrimeKind::Vandals.needs() || taken(p.pos, &out) {
                continue;
            }
            let Some((ri, side, sw, w, along)) = meet(p.pos) else {
                continue;
            };
            // On the pavement side of it.
            let to_walk = (w - p.pos).normalize_or(along.perp() * side);
            let reach = p
                .footprints()
                .iter()
                .map(|f| f.at().distance(p.pos) + f.reach())
                .fold(0.4, f64::max);
            let smasher = p.pos + to_walk * (reach + 0.75);
            let mate = p.pos + to_walk * (reach + 1.5) + along * 1.3;
            if !self.stand_ok(smasher) || !self.stand_ok(mate) {
                continue;
            }
            let Some(ways) = ways(ri, side, sw) else {
                continue;
            };
            let id = self.area.prop_ids.get(i as usize).map_or(0, |d| d.0);
            out.push(CrimePlace {
                key: 0,
                kind: PlaceKind::Prop,
                at: p.pos,
                y: self.ground_at(smasher),
                along,
                inner: -to_walk,
                spots: vec![smasher, mate],
                door: None,
                prop: Some((id, p.kind)),
                ways,
                rough,
            });
        }
        out.into_iter()
            .enumerate()
            .map(|(k, mut c)| {
                c.key = seed.child("place").child(k as u64).0;
                Arc::new(c)
            })
            .collect()
    }

    /// Every crime place within `radius` of `p` (worked out on first ask).
    pub fn crime_places_near(&self, p: DVec2, radius: f64) -> Vec<Arc<CrimePlace>> {
        let (a, b) = (
            cell(p - DVec2::splat(radius), PATCH),
            cell(p + DVec2::splat(radius), PATCH),
        );
        let mut out = Vec::new();
        for x in a.0..=b.0 {
            for z in a.1..=b.1 {
                for c in self.crime_places((x, z)).iter() {
                    if c.at.distance(p) <= radius {
                        out.push(c.clone());
                    }
                }
            }
        }
        out
    }

    /// How many crime patches and crimes are held (memory stays bounded).
    pub fn crime_held(&self) -> (usize, usize) {
        self.crime.lock().expect("crime").held()
    }

    /// What crime happens at `place` in slot `slot`, if any (by the hour
    /// of the game clock then: the same on every machine).
    pub fn crime_kind_at(&self, place: &CrimePlace, slot: i64) -> Option<CrimeKind> {
        if let Some(k) = forced() {
            return (k.place() == place.kind).then_some(k);
        }
        let mid = (slot as f64 + 0.5) * SLOT;
        let hour = clock::hour_at(mid);
        let s = self.seed.child("crime").child(place.key).child(slot as u64);
        if s.child("on").unit() >= rate(place.kind) * place.rough {
            return None;
        }
        let fits: Vec<CrimeKind> = CrimeKind::ALL
            .into_iter()
            .filter(|k| k.place() == place.kind && k.on_at(hour) && place.rough >= k.needs())
            .collect();
        if fits.is_empty() {
            return None;
        }
        let w: Vec<f64> = fits
            .iter()
            .map(|k| k.weight(place.rough).max(0.05))
            .collect();
        Some(fits[s.child("kind").rng().weighted(&w)])
    }

    /// The crimes near `p` with someone of them about at `time` (or about
    /// to be, within `ahead` seconds), nearest first.
    pub fn crimes_near(&self, p: DVec2, radius: f64, time: f64, ahead: f64) -> Vec<Arc<Crime>> {
        let slot = (time / SLOT).floor() as i64;
        let mut out = Vec::new();
        for place in self.crime_places_near(p, radius) {
            for k in [slot - 1, slot] {
                if let Some(c) = self.crime_at(&place, k) {
                    if c.first <= time + ahead && time < c.gone {
                        out.push(c);
                    }
                }
            }
        }
        out.sort_by(|a, b| {
            let da = a.place.at.distance_squared(p);
            let db = b.place.at.distance_squared(p);
            da.total_cmp(&db).then(a.key().cmp(&b.key()))
        });
        out
    }

    /// The tags on walls near `p` at `time`: sprayed, or being sprayed,
    /// and not yet cleaned off (see [`TAG_LIFE`]).
    pub fn tags_near(&self, p: DVec2, radius: f64, time: f64) -> Vec<Tag> {
        let slot = (time / SLOT).floor() as i64;
        let back = (TAG_LIFE / SLOT).ceil() as i64 + 2;
        let mut out = Vec::new();
        for place in self.crime_places_near(p, radius) {
            if place.kind != PlaceKind::Wall {
                continue;
            }
            for k in slot - back..=slot {
                if let Some(t) = self.crime_at(&place, k).and_then(|c| c.tag) {
                    if t.from <= time && t.on(time) {
                        out.push(t);
                    }
                }
            }
        }
        out
    }

    /// The crime at `place` in slot `slot`, if any (planned on first ask).
    pub fn crime_at(&self, place: &Arc<CrimePlace>, slot: i64) -> Option<Arc<Crime>> {
        let key = (place.key, slot);
        if let Some(c) = self.crime.lock().expect("crime").crimes.get(&key) {
            return c;
        }
        let c = self
            .crime_kind_at(place, slot)
            .map(|k| Arc::new(self.plan_crime(place, k, slot)));
        self.crime.lock().expect("crime").crimes.put(key, c.clone());
        c
    }

    fn plan_crime(&self, place: &Arc<CrimePlace>, kind: CrimeKind, slot: i64) -> Crime {
        let seed = self
            .seed
            .child("crime plan")
            .child(place.key)
            .child(slot as u64);
        let mut rng = seed.rng();
        // It starts (the crooks there) somewhere in its hour, leaving room
        // for them to come before and go after.
        let t = slot as f64 * SLOT + rng.range(35.0, SLOT - 20.0);
        // Which way the crooks come from (the victim from the other).
        let a = rng.index(2);
        let crook = kind.crook();
        let look = |k: u64| (seed.child("look").child(k).0 % 251) as u8;
        let crook_id = |k: u64| seed.child("crook").child(k).0;
        let mut parts: Vec<Part> = (0..kind.crooks())
            .map(|k| {
                Part::new(
                    crook_id(k as u64),
                    CrimeRole::Crook(crook, look(k as u64), k),
                )
            })
            .collect();
        let victim_look = (seed.child("victim").0 % LOOKS as u64) as u32;
        let mut victim = Part::new(seed.child("victim id").0, CrimeRole::Victim(victim_look));
        // A path from `from` onto the pavement and along way `w` (from
        // the first of its points ahead of `from`, so nobody steps back).
        let path = |from: DVec2, w: usize| -> Arc<Polyline> {
            let way = &place.ways[w];
            let dir = (way[1] - way[0]).normalize_or(DVec2::X);
            let ahead = (from - way[0]).dot(dir) + 0.3;
            let skip = way
                .iter()
                .position(|q| (*q - way[0]).dot(dir) > ahead)
                .unwrap_or(way.len() - 1);
            let mut pts = vec![from];
            pts.extend(way[skip..].iter().copied());
            Arc::new(Polyline::new(&pts))
        };
        let along_a = if a == 0 { place.along } else { -place.along };
        let p = place.at;
        let mut smash = None;
        let mut tag = None;
        let (on, done);
        match kind {
            CrimeKind::Snatch => {
                // The thief loiters by the wall; the victim comes along with
                // her bag, he grabs it and runs back the way he came, she
                // shouts after him and goes home.
                let lean = place.spots[if a == 0 { 1 } else { 2 }];
                let wait = rng.range(8.0, 22.0);
                let grab = t + wait;
                let thief = &mut parts[0];
                thief
                    .come(&path(lean, a), STROLL, t)
                    .stand(
                        t,
                        grab - 1.2,
                        lean,
                        -place.inner + along_a * 0.3,
                        Deed::Lounging,
                    )
                    .step(grab - 1.2, 1.2, lean, p + along_a * 0.55, Deed::Walking)
                    .stand(
                        grab,
                        grab + 0.9,
                        p + along_a * 0.55,
                        -along_a,
                        Deed::Grabbing,
                    )
                    .go(&path(p + along_a * 0.55, a), RUN, grab + 0.9, Deed::Running)
                    .hold(grab + 0.5, f64::INFINITY, Loot::Handbag);
                victim
                    .come(&path(p, 1 - a), PASSER, grab)
                    .hold(f64::NEG_INFINITY, grab + 0.5, Loot::Handbag)
                    .stand(grab, grab + 1.2, p, along_a, Deed::Cowering)
                    .stand(grab + 1.2, grab + 6.0, p, along_a, Deed::Shouting)
                    .stand(grab + 6.0, grab + 8.0, p, -along_a, Deed::Shaken)
                    .go(&path(p, 1 - a), STROLL, grab + 8.0, Deed::Walking);
                on = t;
                done = grab + 1.0;
            }
            CrimeKind::Hassle => {
                // Two lounge by the wall; a passer-by comes along and they
                // step into the way, jeer and shove; the passer-by gets
                // away at a run, and they laugh and stroll off.
                let wait = rng.range(8.0, 20.0);
                let meet = t + wait;
                let block = p + along_a * 0.85;
                let flank = p + place.inner * 0.75 + along_a * 0.2;
                let spots = [place.spots[1], place.spots[2]];
                for (k, part) in parts.iter_mut().enumerate() {
                    let lean = spots[k];
                    let to = if k == 0 { block } else { flank };
                    part.come(&path(lean, a), STROLL, t - k as f64 * 1.5)
                        .stand(
                            t - k as f64 * 1.5,
                            meet - 1.6,
                            lean,
                            -place.inner,
                            Deed::Lounging,
                        )
                        .step(meet - 1.6, 1.6, lean, to, Deed::Walking);
                    let face = (p - to).normalize_or(-along_a);
                    if k == 0 {
                        part.stand(meet, meet + 3.0, to, face, Deed::Threatening)
                            .stand(meet + 3.0, meet + 4.2, to, face, Deed::Shoving)
                            .stand(meet + 4.2, meet + 8.0, to, face, Deed::Threatening);
                    } else {
                        part.stand(meet, meet + 5.0, to, face, Deed::Laughing)
                            .stand(meet + 5.0, meet + 8.0, to, face, Deed::Threatening);
                    }
                    part.stand(meet + 8.0, meet + 12.0, to, -along_a, Deed::Laughing)
                        .go(
                            &path(to, a),
                            STROLL,
                            meet + 12.0 + k as f64 * 0.8,
                            Deed::Walking,
                        );
                }
                victim
                    .come(&path(p, 1 - a), PASSER, meet)
                    .stand(meet, meet + 8.0, p, along_a, Deed::Cowering)
                    .go(&path(p, 1 - a), HURRY, meet + 8.0, Deed::Hurrying);
                on = t;
                done = meet + 8.0;
            }
            CrimeKind::Shakedown | CrimeKind::Protection => {
                // The two walk up to the shop; the shopkeeper comes out, is
                // leaned on, hands the money over and goes back in, shaken;
                // the two count it and stroll off.
                let door = place.door.unwrap_or(p - place.inner * 0.5);
                let owner = place.spots[0];
                let stands = [place.spots[1], place.spots[2]];
                let loot = if kind == CrimeKind::Protection {
                    Loot::Envelope
                } else {
                    Loot::Cash
                };
                let talk = rng.range(9.0, 14.0);
                let paid = t + 2.0 + talk;
                for (k, part) in parts.iter_mut().enumerate() {
                    let at = stands[k];
                    let face = (owner - at).normalize_or(place.inner);
                    let arrive = t + k as f64 * 0.8;
                    part.come(&path(at, a), STROLL, arrive);
                    if k == 0 {
                        part.stand(arrive, paid, at, face, Deed::Threatening)
                            .stand(paid, paid + 1.0, at, face, Deed::Grabbing)
                            .stand(paid + 1.0, paid + 4.5, at, face, Deed::Counting)
                            .hold(paid + 0.6, f64::INFINITY, loot);
                    } else {
                        part.stand(arrive, paid, at, -place.inner, Deed::Watching)
                            .stand(paid, paid + 4.5, at, face, Deed::Laughing);
                    }
                    part.go(
                        &path(at, 1 - a),
                        STROLL,
                        paid + 4.5 + k as f64,
                        Deed::Walking,
                    );
                }
                let out_door = door - place.inner * 0.15;
                victim
                    .step(t - 0.5, 2.0, out_door, owner, Deed::Walking)
                    .stand(t + 1.5, paid - 1.5, owner, -place.inner, Deed::Cowering)
                    .stand(
                        paid - 1.5,
                        paid + 1.0,
                        owner,
                        -place.inner,
                        Deed::HandingOver,
                    )
                    .hold(paid - 1.5, paid + 0.6, loot)
                    .stand(paid + 1.0, paid + 6.0, owner, -place.inner, Deed::Shaken)
                    .step(paid + 6.0, 2.0, owner, out_door, Deed::Walking);
                on = t;
                done = paid + 1.0;
            }
            CrimeKind::Tagging => {
                // One sprays the gang's sign, the other keeps watch; then
                // they admire it and stroll off.
                let spray = rng.range(20.0, 28.0);
                let [tagger, lookout] = [place.spots[0], place.spots[1]];
                let wall_n = -place.inner;
                let back = tagger + wall_n * 0.9;
                parts[0]
                    .come(&path(tagger, a), STROLL, t)
                    .stand(t, t + spray, tagger, place.inner, Deed::Spraying)
                    .step(t + spray, 0.8, tagger, back, Deed::Walking)
                    .stand(
                        t + spray + 0.8,
                        t + spray + 4.0,
                        back,
                        place.inner,
                        Deed::Laughing,
                    )
                    .go(&path(back, 1 - a), STROLL, t + spray + 4.0, Deed::Walking)
                    .hold(t - 2.0, t + spray + 0.5, Loot::SprayCan);
                parts[1]
                    .come(&path(lookout, a), STROLL, t + 0.8)
                    .stand(t + 0.8, t + spray, lookout, along_a, Deed::Watching)
                    .stand(
                        t + spray,
                        t + spray + 4.0,
                        lookout,
                        place.inner,
                        Deed::Laughing,
                    )
                    .go(
                        &path(lookout, 1 - a),
                        STROLL,
                        t + spray + 4.8,
                        Deed::Walking,
                    );
                let y = place.y + rng.range(1.1, 1.5);
                tag = Some(Tag {
                    key: (place.key, slot),
                    at: DVec3::new(place.at.x, y, place.at.y),
                    normal: wall_n,
                    size: rng.range(1.2, 1.7),
                    image: rng.index(4) as u8,
                    from: t + 1.0,
                    to: t + spray - 0.5,
                });
                on = t;
                done = t + spray;
            }
            CrimeKind::Vandals => {
                // One swings at the thing till it gives; both laugh and
                // walk off.
                let [smasher, mate] = [place.spots[0], place.spots[1]];
                let at_it = (place.at - smasher).normalize_or(place.inner);
                let hit = t + rng.range(3.0, 5.0);
                parts[0]
                    .come(&path(smasher, a), STROLL, t)
                    .stand(t, hit - 1.6, smasher, at_it, Deed::Threatening)
                    .stand(hit - 1.6, hit + 1.4, smasher, at_it, Deed::Smashing)
                    .stand(hit + 1.4, hit + 6.0, smasher, at_it, Deed::Laughing)
                    .go(&path(smasher, 1 - a), STROLL, hit + 6.0, Deed::Walking);
                parts[1]
                    .come(&path(mate, a), STROLL, t + 0.7)
                    .stand(
                        t + 0.7,
                        hit + 1.0,
                        mate,
                        (place.at - mate).normalize_or(at_it),
                        Deed::Threatening,
                    )
                    .stand(
                        hit + 1.0,
                        hit + 6.5,
                        mate,
                        (place.at - mate).normalize_or(at_it),
                        Deed::Laughing,
                    )
                    .go(&path(mate, 1 - a), STROLL, hit + 6.5, Deed::Walking);
                smash = Some(hit);
                on = t;
                done = hit;
            }
        }
        let has_victim = !victim.beats.is_empty();
        if has_victim {
            parts.push(victim);
        }
        let first = parts.iter().map(Part::first).fold(f64::INFINITY, f64::min);
        let gone = parts.iter().map(|p| p.end(0.0)).fold(0.0, f64::max);
        Crime {
            place: place.clone(),
            kind,
            slot,
            seed,
            parts,
            on,
            done,
            first,
            gone,
            smash,
            tag,
        }
    }
}

/// `HW_CRIME=<snatch|hassle|shakedown|protection|tagging|vandals>`: every
/// place of its kind has that crime, every hour (screenshots and tests).
/// Read once.
pub fn forced() -> Option<CrimeKind> {
    static FORCED: std::sync::OnceLock<Option<CrimeKind>> = std::sync::OnceLock::new();
    *FORCED.get_or_init(|| {
        let want = std::env::var("HW_CRIME").ok()?;
        CrimeKind::ALL
            .into_iter()
            .find(|k| format!("{k:?}").eq_ignore_ascii_case(&want))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GeneratedTown;
    use std::collections::BTreeMap;

    fn life() -> StreetLife {
        let t = GeneratedTown::generate(Seed(1), 40_000.0);
        StreetLife::new(t.area)
    }

    /// The middle of the square, and how far round it to look.
    fn middle(life: &StreetLife) -> (DVec2, f64) {
        let a = &life.area;
        ((a.lo + a.hi) * 0.5, (a.hi - a.lo).max_element() * 0.5)
    }

    #[test]
    #[ignore = "daily (slow blocker): scripts/check.sh --daily, and CI's first run of the day"]
    fn same_seed_same_crimes_and_nobody_appears_out_of_nowhere() {
        let a = life();
        let b = StreetLife::new(a.area.clone());
        let (at, r) = middle(&a);
        let mut kinds: BTreeMap<CrimeKind, usize> = BTreeMap::new();
        // A day, every half game hour.
        for k in 0..48 {
            let time = k as f64 * SLOT * 0.5 + 7.0;
            let ca = a.crimes_near(at, r, time, 0.0);
            let cb = b.crimes_near(at, r, time, 0.0);
            assert_eq!(ca.len(), cb.len());
            for (x, y) in ca.iter().zip(&cb) {
                assert_eq!((x.key(), x.kind), (y.key(), y.kind));
                assert_eq!(
                    x.people(&a, time),
                    y.people(&b, time),
                    "same crime, same people"
                );
            }
            for c in &ca {
                *kinds.entry(c.kind).or_default() += 1;
            }
        }
        // Every crime of the day: whoever shows up comes along the
        // pavement from a way off (or out of the shop door) and goes off
        // the same way; nobody pops up at the scene.
        let mut seen = 0;
        for place in a.crime_places_near(at, r) {
            for slot in 0..24 {
                let Some(c) = a.crime_at(&place, slot) else {
                    continue;
                };
                seen += 1;
                for part in &c.parts {
                    let ends = [part.first() + 1e-3, part.end(0.0) - 1e-3];
                    for t in ends {
                        let p = part.at(&a, t).expect("there at its own ends");
                        let flat = DVec2::new(p.pos.x, p.pos.z);
                        let from_door = c.place.door.map_or(f64::INFINITY, |d| d.distance(flat));
                        assert!(
                            flat.distance(c.place.at) > WAY_MIN * 0.7 || from_door < 1.0,
                            "{:?} of a {:?} shows up {:.1} m from it",
                            part.role,
                            c.kind,
                            flat.distance(c.place.at)
                        );
                        assert!(p.pos.is_finite() && p.facing.is_finite());
                    }
                }
                assert!(c.first < c.on && c.on < c.done && c.done < c.gone);
            }
        }
        assert!(seen > 20, "{seen} crimes in a day");
        assert!(kinds.len() >= 4, "kinds: {kinds:?}");
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn crime_keeps_to_rough_streets_and_places_are_clear() {
        let a = life();
        let (at, r) = middle(&a);
        let places = a.crime_places_near(at, r);
        let of = |k: PlaceKind| places.iter().filter(|p| p.kind == k).count();
        for k in [PlaceKind::Pavement, PlaceKind::Door, PlaceKind::Wall] {
            assert!(of(k) >= 3, "{} {k:?} places", of(k));
        }
        for p in &places {
            assert!(p.rough > 0.0);
            for s in &p.spots {
                assert!(a.stand_ok(*s), "a {:?} place's spot is not clear", p.kind);
            }
            for w in &p.ways {
                assert!(w.len() as f64 > WAY_MIN);
                for q in w.iter() {
                    assert!(!a.in_building(*q, 0.2), "a way runs through a house");
                }
            }
            if let Some(h) = a
                .area
                .landmarks
                .iter()
                .find(|l| l.kind == LandmarkKind::HeroHall)
            {
                assert!(h.pos.distance(p.at) > h.plaza + 100.0, "crime by Hero Hall");
            }
        }
        // Rough streets see much more of it than the rest of town.
        let mut by: [(f64, f64); 2] = [(0.0, 0.0); 2];
        for p in &places {
            let rough = (p.rough >= 0.5) as usize;
            for slot in 0..48 {
                by[rough].0 += 1.0;
                by[rough].1 += a.crime_kind_at(p, slot).is_some() as u8 as f64;
            }
        }
        if by[1].0 > 0.0 && by[0].0 > 0.0 {
            assert!(by[1].1 / by[1].0 > 2.0 * by[0].1 / by[0].0, "{by:?}");
        }
    }
}
