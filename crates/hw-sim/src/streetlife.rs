//! Street life: the town's characters. A soapbox prophet warning of the
//! saucers, a theremin player, a chrome spaceman who is really a living
//! statue, an atom juggler, a saucer spotter with binoculars and a foil
//! cap, the newsboy crying the headlines; after dark a stargazer with a
//! telescope, swing dancers round a radio, and the queue at a night club's
//! door. And by day now and then a star trader of the Vael, from a world
//! round another sun, gliding in on a hover disc to sell glowing trinkets.
//!
//! Like the citizens, all of it is a pure function of the town seed and
//! the game time: where the pitches are (worked out per patch of town,
//! only near the camera, and forgotten again), who has a pitch in which
//! two-hour slot of the day, and what everyone there is doing at a given
//! second. Every machine in a co-op game sees the same acts, nothing is
//! sent, and nothing is kept for the parts of town nobody is near.
//!
//! Nobody stands about for nothing. A performer walks in along the
//! pavement, sets up, performs, packs up and walks off at the end of the
//! slot. Passers-by come to watch: they walk up, watch a while, clap,
//! drop a coin in the hat and go on their way. People buy a paper from
//! the newsboy. Night club guests arrive, queue, and are let in one by one
//! by the doorman; now and then someone comes out and goes home.
//!
//! Pitches keep clear of the line people walk along the pavement, of
//! doors, of everything standing in the street (by its solid shape) and of
//! the carriageway, so the act never stands in anyone's way.

use crate::citizens::{Citizens, Escape, LOOKS};
use crate::clock::SECONDS_PER_HOUR;
use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_gen::building::{BuildingPlan, BuildingUse, OpeningKind, Side};
use hw_gen::city::DistrictKind;
use hw_gen::geom::Polyline;
use hw_gen::urban::area::Area;
use hw_gen::urban::landmarks::LandmarkKind;
use hw_gen::urban::props::PropKind;
use hw_gen::urban::world::{WorldBuilding, WorldRoad};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

/// Game seconds one act keeps its pitch: two hours of the day.
pub const SLOT: f64 = 2.0 * SECONDS_PER_HOUR;
/// Pitches are worked out per square patch of town this wide (m).
const PATCH: f64 = 96.0;
/// Pitches keep this far inside their patch (m).
const EDGE: f64 = 5.0;
/// How many patches' pitches are kept at once (the camera's surroundings).
const KEEP_PATCHES: usize = 160;
/// How many acts' plans are kept at once.
const KEEP_SHOWS: usize = 96;
/// Index cells of roads, buildings and props (m).
const CELL: f64 = 32.0;
/// Two pitches at least this far apart (m).
const SPACING: f64 = 24.0;
/// A pitch keeps this far from the line people walk along (m)...
const WALK_CLEAR: f64 = 1.1;
/// ...and an audience's place this far.
const SPOT_WALK_CLEAR: f64 = 0.75;
/// A club queue against the shop fronts keeps this far (shoulders past
/// shoulders).
const QUEUE_WALK_CLEAR: f64 = 0.52;
/// ...and this far from anything walkers step round (they swing out
/// towards the houses round it).
const QUEUE_PROP_CLEAR: f64 = 1.8;
/// From anything solid in the street (m), for the act and for a watcher.
const PROP_CLEAR: f64 = 1.1;
const SPOT_PROP_CLEAR: f64 = 0.55;
/// From a door's middle (m).
const DOOR_CLEAR: f64 = 2.2;
/// How far performers and watchers walk in and out along the pavement (m).
const WALK_IN: f64 = 42.0;
/// Walking pace of performers (with their things) and of watchers (m/s).
const PERFORMER_PACE: f64 = 1.2;
const WATCHER_PACE: f64 = 1.35;
/// Seconds to set up and to pack up.
pub const SETUP: f64 = 5.0;
pub const PACK: f64 = 5.0;
/// Looks of night club guests (two in dinner suits, two in gowns).
pub const GUEST_LOOKS: u8 = 4;

/// What a pitch is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SiteKind {
    /// On a wide pavement in front of shops, against the shop fronts.
    Pavement,
    /// On a square: by a fountain or a statue, or on a landmark's plaza.
    Square,
    /// The door of a night club (a shop front on a main road downtown).
    Door,
}

/// A place an act can set up.
#[derive(Clone, Debug, PartialEq)]
pub struct Site {
    /// Stable for the town: the same pitch every time it is worked out.
    pub key: u64,
    pub kind: SiteKind,
    /// Where the performer stands, and the way they face (the passers-by).
    pub at: DVec2,
    pub face: DVec2,
    /// Height of the ground there.
    pub y: f64,
    /// The audience's places (or a queue's, door first), each with the way
    /// it faces.
    pub spots: Vec<(DVec2, DVec2)>,
    /// A club's door: where guests go in, on the facade.
    pub door: Option<DVec2>,
    /// How busy the neighbourhood is (0 to 1).
    pub busy: f64,
}

/// The acts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Act {
    /// A soapbox prophet in a sandwich board: the saucers are coming.
    Prophet,
    /// A theremin player in a dinner jacket, hands waving over the aerials.
    Theremin,
    /// A living statue: a chrome spaceman on a crate, perfectly still,
    /// who moves when you come close.
    Statue,
    /// A juggler with three glowing atoms.
    Juggler,
    /// A saucer spotter in a foil cap, binoculars on the sky.
    Spotter,
    /// The newsboy crying the headlines; people buy a paper.
    Newsboy,
    /// An amateur astronomer with a brass telescope, after dark.
    Stargazer,
    /// A swing-dancing couple round a portable radio, in the evening.
    Dancers,
    /// The queue at a night club's door, and its doorman.
    Club,
    /// A star trader of the Vael, friendly visitors from round another
    /// sun: rides a hover disc, hawks glowing trinkets from a floating
    /// tray, and people come to buy.
    Trader,
}

impl Act {
    pub const ALL: [Act; 10] = [
        Act::Prophet,
        Act::Theremin,
        Act::Statue,
        Act::Juggler,
        Act::Spotter,
        Act::Newsboy,
        Act::Stargazer,
        Act::Dancers,
        Act::Club,
        Act::Trader,
    ];

    /// What people call it.
    pub fn title(self) -> &'static str {
        match self {
            Act::Prophet => "Soapbox prophet",
            Act::Theremin => "Theremin player",
            Act::Statue => "Living statue",
            Act::Juggler => "Atom juggler",
            Act::Spotter => "Saucer spotter",
            Act::Newsboy => "Newsboy",
            Act::Stargazer => "Stargazer",
            Act::Dancers => "Swing dancers",
            Act::Club => "The Orbit Room",
            Act::Trader => "Vael star trader",
        }
    }

    /// Hours of the day it is on, [from, to) (to may run past midnight).
    fn hours(self) -> &'static [(f64, f64)] {
        match self {
            Act::Prophet => &[(9.0, 20.0)],
            Act::Theremin => &[(10.0, 23.0)],
            Act::Statue => &[(10.0, 18.5)],
            Act::Juggler => &[(11.0, 19.0)],
            Act::Spotter => &[(8.0, 23.5)],
            Act::Newsboy => &[(6.5, 11.0), (16.0, 19.5)],
            Act::Stargazer => &[(20.0, 26.5)],
            Act::Dancers => &[(17.0, 24.5)],
            Act::Club => &[(20.5, 27.0)],
            Act::Trader => &[(10.0, 19.5)],
        }
    }

    /// Is it on at `hour`?
    pub fn on_at(self, hour: f64) -> bool {
        let h = hour.rem_euclid(24.0);
        self.hours()
            .iter()
            .any(|&(a, b)| (h >= a && h < b) || (h + 24.0 >= a && h + 24.0 < b))
    }

    fn weight(self) -> f64 {
        match self {
            Act::Prophet => 1.0,
            Act::Theremin => 1.2,
            Act::Statue => 0.9,
            Act::Juggler => 1.0,
            Act::Spotter => 0.8,
            Act::Newsboy => 1.1,
            Act::Stargazer => 1.3,
            Act::Dancers => 1.2,
            Act::Club => 1.0,
            Act::Trader => 0.8,
        }
    }

    fn fits(self, kind: SiteKind) -> bool {
        match self {
            Act::Club => kind == SiteKind::Door,
            Act::Newsboy => kind == SiteKind::Pavement,
            _ => kind != SiteKind::Door,
        }
    }

    /// How many perform it (the dancers are a couple).
    pub fn performers(self) -> u8 {
        match self {
            Act::Dancers => 2,
            _ => 1,
        }
    }

    /// How many watch it at most (or queue, at the club).
    fn audience(self) -> usize {
        match self {
            Act::Prophet => 2,
            Act::Theremin => 4,
            Act::Statue => 3,
            Act::Juggler => 4,
            Act::Spotter => 1,
            Act::Newsboy => 1,
            Act::Stargazer => 3,
            Act::Dancers => 4,
            Act::Club => 6,
            Act::Trader => 3,
        }
    }

    /// Does the audience leave a coin?
    pub fn takes_coins(self) -> bool {
        matches!(
            self,
            Act::Theremin | Act::Statue | Act::Juggler | Act::Dancers
        )
    }

    /// Stands on something (m): the prophet's soapbox, the statue's crate.
    pub fn lift(self) -> f64 {
        match self {
            Act::Prophet => SOAPBOX,
            Act::Statue => CRATE,
            Act::Trader => HOVER,
            _ => 0.0,
        }
    }
}

/// Height of the prophet's soapbox and of the statue's crate (m).
pub const SOAPBOX: f64 = 0.34;
pub const CRATE: f64 = 0.52;
/// How high the trader's hover disc carries them, its top over the ground
/// (m): all the time, gliding in and out too.
pub const HOVER: f64 = 0.42;

impl Act {
    /// Buyers come up, buy and go (the newsboy's paper, the trader's
    /// trinkets): no watching, no clapping.
    pub fn sells(self) -> bool {
        matches!(self, Act::Newsboy | Act::Trader)
    }

    /// Rides in and out on something (the trader's hover disc): lifted the
    /// whole time, not only while performing.
    pub fn rides(self) -> bool {
        self == Act::Trader
    }
}

/// Who someone is, for whoever draws them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Role {
    /// One of an act's performers (the dancers are 0 and 1; the club's is
    /// its doorman).
    Performer(Act, u8),
    /// A passer-by who stopped: one of the citizens' looks.
    Townsfolk(u32),
    /// A guest dressed for the night club, one of [`GUEST_LOOKS`].
    Guest(u8),
}

/// What someone is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Doing {
    /// Walking in or away (`carrying`: with their things).
    Walking,
    /// Unpacking and setting up.
    Setting,
    /// The act itself.
    Performing,
    /// Between turns: patter for the audience.
    Talking,
    /// Packing up.
    Packing,
    /// Watching the act.
    Watching,
    /// Clapping it.
    Clapping,
    /// Dropping a coin in the hat.
    Tipping,
    /// Buying a paper.
    Buying,
    /// Waiting in the club's queue.
    Queueing,
}

/// Someone of the street life at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StreetPerson {
    /// Stable while they are about (act, part and visit).
    pub id: u64,
    pub role: Role,
    /// Feet on the ground (on the soapbox or crate while performing).
    pub pos: DVec3,
    /// The way they face (unit, in the ground plane).
    pub facing: DVec2,
    /// Walking speed now (0 standing).
    pub speed: f64,
    pub doing: Doing,
    /// Carrying their things (a performer walking in or away).
    pub carrying: bool,
}

/// One act at one pitch in one slot.
#[derive(Clone, Debug)]
pub struct Show {
    pub site: Arc<Site>,
    pub act: Act,
    pub slot: i64,
    seed: Seed,
    /// The performers' (or the doorman's) part.
    lead: Vec<Part>,
    /// Everybody else: watchers, customers, guests.
    crowd: Vec<Part>,
    /// The club's guests, if a club (queue order).
    guests: Vec<Guest>,
    /// When the performance runs: set up from `on`, packed by `off`.
    pub on: f64,
    pub off: f64,
    /// When the last of them is gone.
    gone: f64,
    /// When the first of them comes.
    first: f64,
}

/// Someone's part in a show: walk in, stay at a place, walk away.
#[derive(Clone, Debug)]
struct Part {
    id: u64,
    role: Role,
    /// Their place: the pitch (`None`) or an audience spot.
    spot: Option<usize>,
    /// Sets off towards the place (the walk is `walk_in` backwards).
    arrive: f64,
    walk_in: Option<Arc<Escape>>,
    /// Sets off away.
    leave: f64,
    walk_out: Option<Arc<Escape>>,
    pace: f64,
    /// Customers buy; watchers may tip before they go.
    tips: bool,
}

impl Part {
    fn there(&self) -> f64 {
        self.arrive + self.walk_in.as_ref().map_or(0.0, |e| e.len()) / self.pace
    }

    fn done(&self) -> f64 {
        self.leave + self.walk_out.as_ref().map_or(0.0, |e| e.len()) / self.pace
    }
}

/// A club guest: walks to the back of the queue, moves up, goes in.
#[derive(Clone, Debug)]
struct Guest {
    id: u64,
    look: u8,
    /// Sets off; joins the queue at `joins` (in place `spot`); goes in at `enters`.
    arrive: f64,
    joins: f64,
    walk_in: Option<Arc<Escape>>,
    enters: f64,
}

/// How long a guest takes from the head of the queue to the door (s).
const IN_DOOR: f64 = 2.2;
/// How long the queue takes to move up a place (s).
const SHUFFLE: f64 = 1.4;

struct Index {
    roads: BTreeMap<(i64, i64), Vec<u32>>,
    lines: Vec<Polyline>,
    buildings: BTreeMap<(i64, i64), Vec<u32>>,
    props: BTreeMap<(i64, i64), Vec<u32>>,
}

/// The street life of one town square (the citizens' [`Area`]).
/// The pitches planned in one patch.
type Patch = Arc<Vec<Arc<Site>>>;
/// The act planned at a pitch in a slot, if any.
type Planned = Option<Arc<Show>>;

pub struct StreetLife {
    area: Arc<Area>,
    seed: Seed,
    index: OnceLock<Index>,
    patches: Mutex<Cache<(i64, i64), Patch>>,
    shows: Mutex<Cache<(u64, i64), Planned>>,
    plans: Mutex<Cache<u64, Arc<BuildingPlan>>>,
    /// Street crime's places and crimes (see [`crime`]).
    crime: Mutex<crime::CrimeCache>,
}

/// A small least-recently-used cache.
struct Cache<K, V> {
    map: BTreeMap<K, (V, u64)>,
    tick: u64,
    keep: usize,
}

impl<K: Ord + Clone, V: Clone> Cache<K, V> {
    fn new(keep: usize) -> Self {
        Self {
            map: BTreeMap::new(),
            tick: 0,
            keep,
        }
    }

    fn get(&mut self, k: &K) -> Option<V> {
        self.tick += 1;
        let t = self.tick;
        self.map.get_mut(k).map(|(v, at)| {
            *at = t;
            v.clone()
        })
    }

    fn put(&mut self, k: K, v: V) {
        self.tick += 1;
        self.map.insert(k, (v, self.tick));
        if self.map.len() > self.keep {
            // Drop the oldest quarter at once (rarely, cheaply).
            let mut ages: Vec<u64> = self.map.values().map(|(_, t)| *t).collect();
            ages.sort_unstable();
            let cut = ages[self.map.len() / 4];
            self.map.retain(|_, (_, t)| *t > cut);
        }
    }

    fn len(&self) -> usize {
        self.map.len()
    }
}

fn cell(p: DVec2, size: f64) -> (i64, i64) {
    ((p.x / size).floor() as i64, (p.y / size).floor() as i64)
}

fn around(p: DVec2, r: f64) -> impl Iterator<Item = (i64, i64)> {
    let (a, b) = (
        cell(p - DVec2::splat(r), CELL),
        cell(p + DVec2::splat(r), CELL),
    );
    (a.0..=b.0).flat_map(move |x| (a.1..=b.1).map(move |z| (x, z)))
}

/// How busy streets in a district are with street life.
fn busy(d: DistrictKind) -> f64 {
    match d {
        DistrictKind::Commercial | DistrictKind::Civic => 1.0,
        DistrictKind::Expo => 0.9,
        DistrictKind::Residential => 0.4,
        DistrictKind::AetherWorks => 0.12,
        DistrictKind::Dystopia => 0.0,
    }
}

impl StreetLife {
    /// The street life of `area`. Cheap: everything is worked out when
    /// first asked for, near where it is asked.
    pub fn new(area: Arc<Area>) -> Self {
        let seed = area.seed.child("street life");
        Self {
            area,
            seed,
            index: OnceLock::new(),
            patches: Mutex::new(Cache::new(KEEP_PATCHES)),
            shows: Mutex::new(Cache::new(KEEP_SHOWS)),
            plans: Mutex::new(Cache::new(64)),
            crime: Mutex::new(crime::CrimeCache::new()),
        }
    }

    fn index(&self) -> &Index {
        self.index.get_or_init(|| {
            let a = &self.area;
            let mut roads: BTreeMap<(i64, i64), Vec<u32>> = BTreeMap::new();
            let mut lines = Vec::with_capacity(a.roads.len());
            for (i, r) in a.roads.iter().enumerate() {
                let reach = r.half + r.pavement;
                let mut cells = Vec::new();
                for w in r.pts.windows(2) {
                    let n = (w[0].distance(w[1]) / (CELL * 0.5)).ceil().max(1.0) as usize;
                    for k in 0..=n {
                        let p = w[0].lerp(w[1], k as f64 / n as f64);
                        cells.extend(around(p, reach));
                    }
                }
                cells.sort_unstable();
                cells.dedup();
                for c in cells {
                    roads.entry(c).or_default().push(i as u32);
                }
                lines.push(Polyline::new(&r.pts));
            }
            let mut buildings: BTreeMap<(i64, i64), Vec<u32>> = BTreeMap::new();
            for (i, b) in a.buildings.iter().enumerate() {
                let c = b.site.corners();
                let lo = c.iter().fold(DVec2::splat(f64::MAX), |m, p| m.min(*p));
                let hi = c.iter().fold(DVec2::splat(f64::MIN), |m, p| m.max(*p));
                let (c0, c1) = (cell(lo, CELL), cell(hi, CELL));
                for x in c0.0..=c1.0 {
                    for z in c0.1..=c1.1 {
                        buildings.entry((x, z)).or_default().push(i as u32);
                    }
                }
            }
            let mut props: BTreeMap<(i64, i64), Vec<u32>> = BTreeMap::new();
            for (i, p) in a.props.iter().enumerate() {
                props.entry(cell(p.pos, CELL)).or_default().push(i as u32);
            }
            Index {
                roads,
                lines,
                buildings,
                props,
            }
        })
    }

    fn plan_of(&self, b: u32) -> Arc<BuildingPlan> {
        if let Some(p) = self.plans.lock().expect("plans").get(&(b as u64)) {
            return p;
        }
        let p = Arc::new(BuildingPlan::generate(
            &self.area.buildings[b as usize].site,
        ));
        self.plans.lock().expect("plans").put(b as u64, p.clone());
        p
    }

    /// The street doors of building `b`, on the ground (town frame).
    fn doors_of(&self, b: u32) -> Vec<DVec2> {
        let plan = self.plan_of(b);
        let s = &plan.site;
        plan.openings
            .iter()
            .filter(|o| o.floor == 0 && matches!(o.kind, OpeningKind::Door | OpeningKind::Entrance))
            .map(|o| {
                let o2 = plan.floors[0].outline;
                let local = match o.side {
                    Side::Front => DVec2::new(o.along, o2.min.y),
                    Side::Back => DVec2::new(o.along, o2.max.y),
                    Side::Left => DVec2::new(o2.min.x, o.along),
                    Side::Right => DVec2::new(o2.max.x, o.along),
                };
                s.origin + s.along * local.x + s.inward * local.y
            })
            .collect()
    }

    /// How busy the town is with street life at `p` (0 to 1): downtown and
    /// the old town most, the outskirts a little, never in the villains'
    /// quarter or among the works.
    fn busy_at(&self, p: DVec2) -> f64 {
        let ix = self.index();
        let mut near: Option<(f64, DistrictKind)> = None;
        for c in around(p, 40.0) {
            for &b in ix.buildings.get(&c).into_iter().flatten() {
                let s = &self.area.buildings[b as usize].site;
                let d = (s.origin + s.along * s.size.x * 0.5).distance(p);
                if near.is_none_or(|(e, _)| d < e) {
                    near = Some((d, s.district));
                }
            }
        }
        if let Some((_, d)) = near {
            if matches!(d, DistrictKind::Dystopia | DistrictKind::AetherWorks) {
                return busy(d);
            }
        }
        let t = &self.area.town;
        let Some(piece) = t
            .pieces
            .iter()
            .find(|pc| hw_gen::urban::contains(&pc.face.pts, p))
        else {
            return 0.0;
        };
        let centre = t.growth.centres.iter().any(|c| match c.kind {
            hw_gen::urban::growth::CentreKind::Business => c.pos.distance(p) < 600.0,
            hw_gen::urban::growth::CentreKind::Core => c.pos.distance(p) < 380.0,
            hw_gen::urban::growth::CentreKind::Village => false,
        });
        if centre {
            1.0
        } else if piece.era == hw_gen::urban::growth::Era::Old {
            0.85
        } else {
            0.3
        }
    }

    /// Height of the ground people stand on at `p`: a landmark's plinth,
    /// else the pavement or ground.
    pub fn ground_at(&self, p: DVec2) -> f64 {
        for l in &self.area.landmarks {
            if l.pos.distance(p) <= l.plaza {
                if let Some(y) = l.on_plinth(p) {
                    return y;
                }
            }
        }
        self.area.surface(p)
    }

    fn in_building(&self, p: DVec2, r: f64) -> bool {
        let ix = self.index();
        around(p, r).any(|c| {
            ix.buildings.get(&c).is_some_and(|l| {
                l.iter().any(|&i| {
                    let s = &self.area.buildings[i as usize].site;
                    let d = p - s.origin;
                    let (x, z) = (d.dot(s.along), d.dot(s.inward));
                    x > -r && x < s.size.x + r && z > -r && z < s.size.y + r
                })
            })
        })
    }

    /// Distance from `p` to the nearest solid thing standing in the street.
    fn prop_distance(&self, p: DVec2, within: f64) -> f64 {
        let ix = self.index();
        let mut best = f64::INFINITY;
        for c in around(p, within + 6.0) {
            if let Some(l) = ix.props.get(&c) {
                for &i in l {
                    let prop = &self.area.props[i as usize];
                    if prop.pos.distance(p) > within + 8.0 {
                        continue;
                    }
                    for f in prop.footprints() {
                        best = best.min(f.distance(p));
                    }
                }
            }
        }
        for l in &self.area.landmarks {
            if l.pos.distance(p) < l.plaza + 60.0 {
                for f in l.footprints() {
                    best = best.min(f.distance(p));
                }
            }
        }
        best
    }

    /// Distance from `p` to the nearest line people walk along a pavement
    /// (both sides of every street near), and whether `p` stands on a
    /// carriageway or on another street than `own`'s pavement.
    fn walk_distance(&self, p: DVec2, own: Option<u32>) -> (f64, bool) {
        let ix = self.index();
        let mut cells: Vec<u32> = ix.roads.get(&cell(p, CELL)).cloned().unwrap_or_default();
        cells.sort_unstable();
        cells.dedup();
        let mut best = f64::INFINITY;
        let mut on_road = false;
        for i in cells {
            let r: &WorldRoad = &self.area.roads[i as usize];
            let (s, d) = ix.lines[i as usize].nearest(p);
            if d < r.half + 0.3 {
                on_road = true;
            }
            if Some(i) != own && d < r.half + r.pavement + 0.4 {
                on_road = true;
            }
            best = best.min((d - r.walk_offset()).abs());
            // The ends of a street: people cross there.
            let len = ix.lines[i as usize].len();
            if s < 0.5 || s > len - 0.5 {
                best = best.min(d.max(0.0) - r.half - r.pavement);
            }
        }
        (best.max(0.0), on_road)
    }

    /// Can someone (radius `r`) stand at `p`, `walk` from the walking
    /// line, `prop` from anything solid, clear of `doors`?
    fn free(
        &self,
        p: DVec2,
        r: f64,
        walk: f64,
        prop: f64,
        own: Option<u32>,
        doors: &[DVec2],
    ) -> bool {
        let plinth = self
            .area
            .landmarks
            .iter()
            .any(|l| l.pos.distance(p) <= l.plaza);
        if !plinth && self.area.ground(p) < hw_gen::urban::world::DECK + 0.2 {
            return false;
        }
        let (w, on_road) = self.walk_distance(p, own);
        !on_road
            && w >= walk
            && !self.in_building(p, r)
            && self.prop_distance(p, prop + 1.0) >= prop
            && doors.iter().all(|d| d.distance(p) >= DOOR_CLEAR)
    }

    /// The pitches in the patch `key`.
    fn patch(&self, key: (i64, i64)) -> Arc<Vec<Arc<Site>>> {
        if let Some(p) = self.patches.lock().expect("patches").get(&key) {
            return p;
        }
        let sites = Arc::new(self.plan_patch(key));
        self.patches
            .lock()
            .expect("patches")
            .put(key, sites.clone());
        sites
    }

    fn plan_patch(&self, key: (i64, i64)) -> Vec<Arc<Site>> {
        let lo = DVec2::new(key.0 as f64, key.1 as f64) * PATCH;
        let hi = lo + DVec2::splat(PATCH);
        // Pitches keep off the patch's edge, so two patches' pitches never
        // stand on top of each other.
        let inside = |p: DVec2| {
            p.x >= lo.x + EDGE && p.y >= lo.y + EDGE && p.x < hi.x - EDGE && p.y < hi.y - EDGE
        };
        let seed = self.seed.child("patch").cell(key.0, key.1);
        let ix = self.index();
        let mut out: Vec<Site> = Vec::new();
        let taken = |p: DVec2, out: &Vec<Site>| out.iter().any(|s| s.at.distance(p) < SPACING);
        // Buildings whose front's middle is in the patch.
        let mut list: Vec<u32> = Vec::new();
        let (c0, c1) = (cell(lo, CELL), cell(hi, CELL));
        for x in c0.0..=c1.0 {
            for z in c0.1..=c1.1 {
                if let Some(l) = ix.buildings.get(&(x, z)) {
                    list.extend(l);
                }
            }
        }
        list.sort_unstable();
        list.dedup();
        // Night clubs: shop fronts on main roads downtown.
        for b in list {
            let w: &WorldBuilding = &self.area.buildings[b as usize];
            let s = &w.site;
            let mid = s.origin + s.along * s.size.x * 0.5;
            if !inside(mid)
                || s.usage != BuildingUse::ShopsAndOffices
                || s.floors < 3
                || s.size.x < 10.0
                || !matches!(s.district, DistrictKind::Commercial | DistrictKind::Civic)
            {
                continue;
            }
            let mut rng = seed.child(b as u64).rng();
            if !rng.chance(0.2) {
                continue;
            }
            let Some((ri, kerb)) = self.road_in_front(mid - s.inward * 0.5) else {
                continue;
            };
            let road = &self.area.roads[ri as usize];
            if road.pavement < 2.0 || kerb > road.pavement + 0.8 {
                continue;
            }
            let doors = self.doors_near(mid, 14.0);
            if let Some(site) = self.club(b, ri, &doors, &mut rng) {
                if !taken(site.at, &out) {
                    out.push(site);
                }
            }
        }
        // Open ground just off a pavement (a square, a park's edge, a gap
        // between buildings): the act faces the passers-by, the audience
        // stands beside it, nobody on the pavement itself.
        let mut roads: Vec<u32> = Vec::new();
        for x in c0.0..=c1.0 {
            for z in c0.1..=c1.1 {
                if let Some(l) = ix.roads.get(&(x, z)) {
                    roads.extend(l);
                }
            }
        }
        roads.sort_unstable();
        roads.dedup();
        for ri in roads {
            let road = &self.area.roads[ri as usize];
            if road.pavement < 1.2 || road.pts.len() < 2 {
                continue;
            }
            let line = &ix.lines[ri as usize];
            let len = line.len();
            let mut rng = seed.child("edge").child(road.id.0).rng();
            let mut d = rng.range(8.0, 30.0);
            while d < len - 8.0 {
                let (q, dir) = line.sample(d);
                d += rng.range(22.0, 40.0);
                for side in [-1.0, 1.0] {
                    let n = dir.perp() * side;
                    let at = q + n * (road.half + road.pavement + 1.3);
                    if !inside(at) || taken(at, &out) {
                        continue;
                    }
                    let busy = self.busy_at(at);
                    if busy <= 0.0 || !rng.chance(0.35 + 0.5 * busy) {
                        continue;
                    }
                    // Open: no wall close behind or beside.
                    if self.in_building(at, 3.0)
                        || !self.free(at, 0.6, WALK_CLEAR + 0.4, PROP_CLEAR, None, &[])
                    {
                        continue;
                    }
                    let face = -n;
                    let mut spots = Vec::new();
                    for (dx, back) in [(1.5, -0.3), (-1.5, -0.3), (2.5, 0.35), (-2.5, 0.35)] {
                        let p = at + dir * dx + n * back;
                        if !self.in_building(p, 0.8)
                            && self.free(p, 0.3, SPOT_WALK_CLEAR + 0.4, SPOT_PROP_CLEAR, None, &[])
                        {
                            let f = ((at - p).normalize_or_zero() + face * 0.35).normalize_or(face);
                            spots.push((p, f));
                        }
                    }
                    if spots.len() < 2 {
                        continue;
                    }
                    out.push(Site {
                        key: 0,
                        kind: SiteKind::Pavement,
                        at,
                        face,
                        y: self.ground_at(at),
                        spots,
                        door: None,
                        busy,
                    });
                }
            }
        }
        // Squares: by fountains and statues.
        let mut props: Vec<u32> = Vec::new();
        for x in c0.0..=c1.0 {
            for z in c0.1..=c1.1 {
                if let Some(l) = ix.props.get(&(x, z)) {
                    props.extend(l);
                }
            }
        }
        props.sort_unstable();
        for i in props {
            let p = &self.area.props[i as usize];
            if !inside(p.pos) || !matches!(p.kind, PropKind::Fountain | PropKind::Statue(_)) {
                continue;
            }
            let reach = p
                .footprints()
                .iter()
                .map(|f| f.at().distance(p.pos) + f.reach())
                .fold(0.0, f64::max);
            let mut rng = seed.child("square").child(i as u64).rng();
            let start = rng.range(0.0, std::f64::consts::TAU);
            for k in 0..8 {
                let a = start + k as f64 * std::f64::consts::TAU / 8.0;
                let dir = DVec2::new(hw_core::det::cos(a), hw_core::det::sin(a));
                let at = p.pos + dir * (reach + 1.6);
                if let Some(site) = self.square(at, dir, 0.85, &mut out) {
                    out.push(site);
                    break;
                }
            }
        }
        // Landmark plazas: the civic plaza round the Aether Spire, Hero
        // Hall's forecourt.
        for l in &self.area.landmarks {
            if !matches!(l.kind, LandmarkKind::AetherSpire | LandmarkKind::HeroHall)
                || l.base - l.low > 0.4
            {
                continue;
            }
            let mut rng = seed.child("plaza").child(l.kind as u64).rng();
            let start = rng.range(0.0, std::f64::consts::TAU);
            for k in 0..12 {
                let a = start + k as f64 * std::f64::consts::TAU / 12.0;
                let dir = DVec2::new(hw_core::det::cos(a), hw_core::det::sin(a));
                let at = l.pos + dir * (l.plaza - 4.5);
                if !inside(at) {
                    continue;
                }
                // Facing in, towards the landmark, with the audience
                // round it on the plaza.
                if let Some(site) = self.square(at, -dir, 1.0, &mut out) {
                    out.push(site);
                }
            }
        }
        out.into_iter()
            .enumerate()
            .map(|(k, mut s)| {
                s.key = seed.child("site").child(k as u64).0;
                Arc::new(s)
            })
            .collect()
    }

    /// A pitch on a square at `at`, the performer facing `face`, the
    /// audience in an arc in front.
    fn square(&self, at: DVec2, face: DVec2, busy_here: f64, out: &mut [Site]) -> Option<Site> {
        if out.iter().any(|s| s.at.distance(at) < SPACING) {
            return None;
        }
        if !self.free(at, 0.6, WALK_CLEAR + 0.5, PROP_CLEAR, None, &[]) {
            return None;
        }
        let mut spots = Vec::new();
        for deg in [0.0f64, -32.0, 32.0, -60.0, 60.0] {
            let a = deg.to_radians();
            let (sa, ca) = hw_core::det::sin_cos(a);
            let d = DVec2::new(face.x * ca - face.y * sa, face.x * sa + face.y * ca);
            let q = at + d * 3.0;
            if self.free(q, 0.3, SPOT_WALK_CLEAR, SPOT_PROP_CLEAR, None, &[]) {
                spots.push((q, -d));
            }
        }
        if spots.len() < 2 {
            return None;
        }
        Some(Site {
            key: 0,
            kind: SiteKind::Square,
            at,
            face,
            y: self.ground_at(at),
            spots,
            door: None,
            busy: busy_here,
        })
    }

    /// The night club pitch at building `b`'s street door, if it has one
    /// with room for a doorman and a queue.
    fn club(&self, b: u32, ri: u32, doors: &[DVec2], rng: &mut hw_core::Rng) -> Option<Site> {
        let s = &self.area.buildings[b as usize].site;
        let out_dir = -s.inward;
        let door = self
            .doors_of(b)
            .into_iter()
            .filter(|d| (d - s.origin).dot(s.inward).abs() < 0.6)
            .min_by(|a, b| {
                let m = s.origin + s.along * s.size.x * 0.5;
                a.distance(m).total_cmp(&b.distance(m))
            })?;
        let others: Vec<DVec2> = doors
            .iter()
            .copied()
            .filter(|d| d.distance(door) > 0.5)
            .collect();
        // The doorman on one side of the door, the queue along the front
        // on the other, door first.
        let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
        let at = door + s.along * (1.25 * side) + out_dir * 0.4;
        if !self.free(
            at,
            0.25,
            QUEUE_WALK_CLEAR,
            QUEUE_PROP_CLEAR,
            Some(ri),
            &others,
        ) {
            return None;
        }
        let mut spots = Vec::new();
        for k in 0..6 {
            let q = door - s.along * (side * (1.15 + 0.78 * k as f64)) + out_dir * 0.35;
            if !self.free(
                q,
                0.2,
                QUEUE_WALK_CLEAR,
                QUEUE_PROP_CLEAR,
                Some(ri),
                &others,
            ) {
                break;
            }
            spots.push((q, s.along * side));
        }
        if spots.len() < 3 {
            return None;
        }
        Some(Site {
            key: 0,
            kind: SiteKind::Door,
            at,
            face: (out_dir - s.along * side * 0.6).normalize(),
            y: self.ground_at(at),
            spots,
            door: Some(door),
            busy: busy(s.district),
        })
    }

    /// Street doors of buildings near `p`.
    fn doors_near(&self, p: DVec2, r: f64) -> Vec<DVec2> {
        let ix = self.index();
        let mut list: Vec<u32> = around(p, r)
            .filter_map(|c| ix.buildings.get(&c))
            .flatten()
            .copied()
            .collect();
        list.sort_unstable();
        list.dedup();
        list.into_iter()
            .filter(|&b| {
                let s = &self.area.buildings[b as usize].site;
                let m = s.origin + s.along * s.size.x * 0.5;
                m.distance(p) < r + s.size.x * 0.5
            })
            .flat_map(|b| self.doors_of(b))
            .collect()
    }

    /// The street whose pavement `p` is nearest, and how far from its kerb.
    fn road_in_front(&self, p: DVec2) -> Option<(u32, f64)> {
        let ix = self.index();
        let list = ix.roads.get(&cell(p, CELL))?;
        list.iter()
            .map(|&i| {
                let (_, d) = ix.lines[i as usize].nearest(p);
                (i, d - self.area.roads[i as usize].half)
            })
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
    }

    /// Every pitch within `radius` of `p` (worked out on first ask).
    pub fn sites_near(&self, p: DVec2, radius: f64) -> Vec<Arc<Site>> {
        let (a, b) = (
            cell(p - DVec2::splat(radius), PATCH),
            cell(p + DVec2::splat(radius), PATCH),
        );
        let mut out = Vec::new();
        for x in a.0..=b.0 {
            for z in a.1..=b.1 {
                for s in self.patch((x, z)).iter() {
                    if s.at.distance(p) <= radius {
                        out.push(s.clone());
                    }
                }
            }
        }
        out
    }

    /// How many patches and shows are held (memory stays bounded).
    pub fn held(&self) -> (usize, usize) {
        (
            self.patches.lock().expect("patches").len(),
            self.shows.lock().expect("shows").len(),
        )
    }

    /// What is on at `site` in slot `slot`, given the hour of day at `time`.
    pub fn act_at(&self, site: &Site, slot: i64, hour_now: f64, time: f64) -> Option<Act> {
        let mid = (slot as f64 + 0.5) * SLOT;
        let hour = (hour_now + (mid - time) / SECONDS_PER_HOUR).rem_euclid(24.0);
        let s = self.seed.child(site.key).child(slot as u64);
        let night = !(6.0..20.0).contains(&hour);
        let base = match site.kind {
            SiteKind::Pavement => 0.38,
            SiteKind::Square => 0.75,
            SiteKind::Door => 0.95,
        };
        let busy = if site.kind == SiteKind::Door {
            1.0
        } else {
            site.busy
        };
        let occ = base
            * busy
            * if night && site.kind != SiteKind::Door {
                0.6
            } else {
                1.0
            };
        if s.child("on").unit() >= occ {
            return None;
        }
        let fits: Vec<Act> = Act::ALL
            .into_iter()
            .filter(|a| a.fits(site.kind) && a.on_at(hour))
            .collect();
        if fits.is_empty() {
            return None;
        }
        let weights: Vec<f64> = fits.iter().map(|a| a.weight()).collect();
        Some(fits[s.child("act").rng().weighted(&weights)])
    }

    /// The shows near `p` at `time` (with someone there now or soon), the
    /// hour of day now being `hour`.
    pub fn shows_near(
        &self,
        citizens: &Citizens,
        p: DVec2,
        radius: f64,
        time: f64,
        hour: f64,
    ) -> Vec<Arc<Show>> {
        let slot = (time / SLOT).floor() as i64;
        let mut out = Vec::new();
        for site in self.sites_near(p, radius) {
            for k in [slot - 1, slot] {
                let key = (site.key, k);
                let cached = self.shows.lock().expect("shows").get(&key);
                let show = match cached {
                    Some(s) => s,
                    None => {
                        let s = self.act_at(&site, k, hour, time).map(|act| {
                            Arc::new(self.plan_show(citizens, &site, act, k, hour, time))
                        });
                        self.shows.lock().expect("shows").put(key, s.clone());
                        s
                    }
                };
                if let Some(s) = show {
                    if s.first <= time + 1.0 && time < s.gone {
                        out.push(s);
                    }
                }
            }
        }
        out.sort_by(|a, b| {
            let da = a.site.at.distance_squared(p);
            let db = b.site.at.distance_squared(p);
            da.total_cmp(&db).then(a.site.key.cmp(&b.site.key))
        });
        out
    }

    /// A way off from `from` along the pavement, `ahead` (+1/-1) one way
    /// or the other along the street, clear of everything standing there.
    fn walk_off(
        &self,
        citizens: &Citizens,
        from: DVec2,
        along: DVec2,
        ahead: f64,
    ) -> Option<Arc<Escape>> {
        let e = citizens.escape(from, from - along * ahead, WALK_IN)?;
        // The way over to the pavement must be clear (it is straight).
        let n = (e.len() / 0.5).ceil() as usize;
        for k in 1..n.min(40) {
            let (q, _) = citizens.escape_at(&e, k as f64 * 0.5);
            let q = DVec2::new(q.x, q.z);
            if q.distance(from) < 0.6 {
                continue;
            }
            if self.in_building(q, 0.2) || self.prop_distance(q, 0.5) < 0.25 {
                return None;
            }
        }
        (e.len() > 4.0).then(|| Arc::new(e))
    }

    fn plan_show(
        &self,
        citizens: &Citizens,
        site: &Arc<Site>,
        act: Act,
        slot: i64,
        hour: f64,
        time: f64,
    ) -> Show {
        let seed = self.seed.child("show").child(site.key).child(slot as u64);
        let mut rng = seed.rng();
        let t0 = slot as f64 * SLOT;
        // The street's direction where the act is: across its face.
        let along = DVec2::new(-site.face.y, site.face.x);
        let mut lead = Vec::new();
        let mut crowd = Vec::new();
        let mut guests = Vec::new();
        // The same act the slot before or after (the club's long night):
        // its people stay on instead of leaving and coming back.
        let before = self.act_at(site, slot - 1, hour, time) == Some(act) && act == Act::Club;
        let after = self.act_at(site, slot + 1, hour, time) == Some(act) && act == Act::Club;
        let arrive = if before {
            t0
        } else {
            t0 + rng.range(0.0, 30.0)
        };
        let leave = if after {
            t0 + SLOT + 60.0
        } else {
            t0 + SLOT - rng.range(8.0, 30.0)
        };
        for k in 0..act.performers() {
            let at = performer_at(site, act, k);
            let ahead = if rng.chance(0.5) { 1.0 } else { -1.0 };
            let walk_in = if before {
                None
            } else {
                self.walk_off(citizens, at, along, ahead)
            };
            let walk_out = if after {
                None
            } else {
                self.walk_off(citizens, at, along, -ahead)
            };
            lead.push(Part {
                id: seed.child("lead").child(k as u64).0,
                role: Role::Performer(act, k),
                spot: None,
                arrive: arrive + k as f64 * 0.8,
                walk_in,
                leave,
                walk_out,
                pace: PERFORMER_PACE,
                tips: false,
            });
        }
        let on = lead.iter().map(Part::there).fold(arrive, f64::max);
        let off = leave;
        if act == Act::Club {
            guests = self.plan_guests(citizens, site, &seed, on, off);
            // Now and then someone comes out and goes home.
            let n = rng.range_int(1, 4);
            for j in 0..n {
                let t = rng.range(on + 20.0, (off - 40.0).max(on + 21.0));
                let door = site.door.unwrap_or(site.at);
                let from = door + site.face * 0.6;
                let ahead = if rng.chance(0.5) { 1.0 } else { -1.0 };
                // Out of the door and straight off home.
                crowd.push(Part {
                    id: seed.child("leaver").child(j as u64).0,
                    role: Role::Guest(rng.range_int(0, GUEST_LOOKS as i64) as u8),
                    spot: None,
                    arrive: t,
                    walk_in: None,
                    leave: t,
                    walk_out: self.walk_off(citizens, from, along, ahead),
                    pace: WATCHER_PACE,
                    tips: false,
                });
            }
        } else {
            // The audience: passers-by stop at each place in turn.
            let room = act.audience().min(site.spots.len());
            for spot in 0..room {
                let q = site.spots[spot].0;
                let ways = [
                    self.walk_off(citizens, q, along, 1.0),
                    self.walk_off(citizens, q, along, -1.0),
                ];
                if ways.iter().all(Option::is_none) {
                    continue;
                }
                let mut srng = seed.child("spot").child(spot as u64).rng();
                let mut t = on + srng.range(5.0, 70.0) + spot as f64 * 12.0;
                let mut visit = 0u64;
                while t < off - 40.0 && visit < 12 {
                    let (stay, gap) = match act {
                        Act::Newsboy => (srng.range(4.0, 7.0), srng.range(30.0, 90.0)),
                        // A trinket wants a good look first.
                        Act::Trader => (srng.range(9.0, 20.0), srng.range(25.0, 80.0)),
                        Act::Spotter => (srng.range(20.0, 45.0), srng.range(40.0, 100.0)),
                        _ => (srng.range(45.0, 120.0), srng.range(15.0, 60.0)),
                    };
                    let a = srng.index(2);
                    let (win, wout) = (
                        ways[a].clone().or(ways[1 - a].clone()),
                        ways[1 - a].clone().or(ways[a].clone()),
                    );
                    let walk = win.as_ref().map_or(0.0, |e| e.len()) / WATCHER_PACE;
                    let there = t + walk;
                    let go = (there + stay).min(off - srng.range(0.0, 4.0));
                    if go <= there + 3.0 {
                        break;
                    }
                    let look = (seed.child("look").child(spot as u64).child(visit).0 % LOOKS as u64)
                        as u32;
                    crowd.push(Part {
                        id: seed.child("visit").child(spot as u64).child(visit).0,
                        role: Role::Townsfolk(look),
                        spot: Some(spot),
                        arrive: t,
                        walk_in: win,
                        leave: go,
                        walk_out: wout.clone(),
                        pace: WATCHER_PACE,
                        tips: act.sells() || (act.takes_coins() && srng.chance(0.55)),
                    });
                    let back = wout.as_ref().map_or(0.0, |e| e.len()) / WATCHER_PACE;
                    t = go + back.min(20.0) + gap;
                    visit += 1;
                }
            }
        }
        let first = lead
            .iter()
            .map(|p| p.arrive)
            .chain(crowd.iter().map(|p| p.arrive))
            .chain(guests.iter().map(|g| g.arrive))
            .fold(f64::INFINITY, f64::min);
        let gone = lead
            .iter()
            .map(Part::done)
            .chain(crowd.iter().map(Part::done))
            .chain(guests.iter().map(|g| g.enters + IN_DOOR))
            .fold(0.0, f64::max);
        Show {
            site: site.clone(),
            act,
            slot,
            seed,
            lead,
            crowd,
            guests,
            on,
            off,
            gone,
            first,
        }
    }

    /// The club's guests: one every half minute or so, to the back of the
    /// queue; the doorman lets the one at the front in every ten to twenty
    /// seconds. The queue never grows past its places (late comers wait
    /// for room), and it is empty by the end of the slot.
    fn plan_guests(
        &self,
        citizens: &Citizens,
        site: &Arc<Site>,
        seed: &Seed,
        on: f64,
        off: f64,
    ) -> Vec<Guest> {
        let mut rng = seed.child("guests").rng();
        let along = DVec2::new(-site.face.y, site.face.x);
        let places = site.spots.len();
        let back = site.spots[places - 1].0;
        let ways = [
            self.walk_off(citizens, back, along, 1.0),
            self.walk_off(citizens, back, along, -1.0),
        ];
        let mut out: Vec<Guest> = Vec::new();
        let mut t = on - rng.range(0.0, 20.0);
        let mut last_in = on;
        let mut k = 0u64;
        while t < off - 70.0 && k < 40 {
            let a = rng.index(2);
            let walk_in = ways[a].clone().or(ways[1 - a].clone());
            let walk = walk_in.as_ref().map_or(0.0, |e| e.len()) / WATCHER_PACE;
            let mut joins = t + walk;
            let n = out.len();
            // In the order they come: nobody joins ahead of someone there
            // before them.
            if let Some(prev) = out.last() {
                joins = joins.max(prev.joins + 2.0);
            }
            // Room in the queue: the one `places` ahead must be in.
            if n >= places {
                joins = joins.max(out[n - places].enters + SHUFFLE);
            }
            let enters = (joins + rng.range(6.0, 14.0)).max(last_in + rng.range(9.0, 20.0));
            last_in = enters;
            let arrive = joins - walk;
            out.push(Guest {
                id: seed.child("guest").child(k).0,
                look: rng.range_int(0, GUEST_LOOKS as i64) as u8,
                arrive,
                joins,
                walk_in,
                enters,
            });
            t += rng.range(14.0, 40.0);
            k += 1;
        }
        // Everyone in before the doorman goes (they leave at `off`).
        let mut cut = off - 2.0;
        for g in out.iter_mut().rev() {
            if g.enters > cut {
                g.enters = cut;
            }
            cut = g.enters - 3.0;
        }
        // Those who would not get in by then do not come at all.
        out.retain(|g| g.enters >= g.joins + SHUFFLE);
        out
    }
}

/// Where performer `k` of `act` stands: the dancers side by side.
fn performer_at(site: &Site, act: Act, k: u8) -> DVec2 {
    if act == Act::Dancers {
        let along = DVec2::new(-site.face.y, site.face.x);
        site.at + along * if k == 0 { -0.45 } else { 0.45 } + site.face * 0.3
    } else {
        site.at
    }
}

/// Clapping now and then: about three seconds in every twenty-five or so,
/// at a moment of its own for everyone.
fn clapping(id: u64, t: f64) -> bool {
    let period = 22.0 + (id % 13) as f64;
    let phase = (id >> 8) as f64 % period;
    (t + phase).rem_euclid(period) < 3.0
}

impl Show {
    /// The act's key: its pitch and slot.
    pub fn key(&self) -> (u64, i64) {
        (self.site.key, self.slot)
    }

    /// A number that varies between shows (for little things like the
    /// juggler's rhythm).
    pub fn seed(&self) -> u64 {
        self.seed.0
    }

    /// How far set up the act's things are at `t`: 0 packed (or carried),
    /// 1 standing ready.
    pub fn gear(&self, t: f64) -> f64 {
        let on = self.on;
        let off = self.off;
        if t < on || t >= off {
            return 0.0;
        }
        ((t - on) / SETUP).min((off - t) / PACK).clamp(0.0, 1.0)
    }

    /// Whether the club's door stands open at `t`: a guest going in or
    /// someone coming out.
    pub fn door_open(&self, t: f64) -> bool {
        self.guests
            .iter()
            .any(|g| t >= g.enters + 0.3 && t < g.enters + IN_DOOR + 0.8)
            || (self.act == Act::Club
                && self
                    .crowd
                    .iter()
                    .any(|p| t >= p.arrive - 1.0 && t < p.arrive + 1.2))
    }

    /// Where performer `k` stands to perform (on the ground plan).
    pub fn pitch(&self, k: u8) -> DVec2 {
        performer_at(&self.site, self.act, k)
    }

    /// Whether anyone is about at `t`.
    pub fn active(&self, t: f64) -> bool {
        self.first <= t && t < self.gone
    }

    /// Everyone of the show at `time`.
    pub fn people(&self, life: &StreetLife, citizens: &Citizens, time: f64) -> Vec<StreetPerson> {
        let mut out = Vec::new();
        for (k, p) in self.lead.iter().enumerate() {
            let at = performer_at(&self.site, self.act, k as u8);
            if let Some(mut s) = self.part_at(life, citizens, p, at, self.site.face, time) {
                if s.doing == Doing::Watching {
                    s.doing = self.performing(time, k as u8);
                    // Up on the soapbox or crate while the act is on.
                    if matches!(s.doing, Doing::Performing | Doing::Talking) && !self.act.rides() {
                        s.pos.y += self.act.lift();
                    }
                }
                // On the hover disc the whole time.
                if self.act.rides() {
                    s.pos.y += self.act.lift();
                }
                out.push(s);
            }
        }
        for p in &self.crowd {
            let (at, face) = match p.spot {
                Some(i) if i < self.site.spots.len() => self.site.spots[i],
                _ => (
                    self.site.door.unwrap_or(self.site.at) + self.site.face * 0.6,
                    self.site.face,
                ),
            };
            if let Some(mut s) = self.part_at(life, citizens, p, at, face, time) {
                if s.doing == Doing::Watching {
                    s.doing = if p.tips && time > p.leave - 3.0 {
                        if self.act.sells() {
                            Doing::Buying
                        } else {
                            Doing::Tipping
                        }
                    } else if self.act.sells() {
                        Doing::Buying
                    } else if self.act.takes_coins() && clapping(p.id, time) {
                        Doing::Clapping
                    } else {
                        Doing::Watching
                    };
                }
                out.push(s);
            }
        }
        for g in &self.guests {
            if let Some(s) = self.guest_at(life, citizens, g, time) {
                out.push(s);
            }
        }
        out
    }

    /// What performer `k` is doing during the act at `t`.
    fn performing(&self, t: f64, k: u8) -> Doing {
        if t < self.on + SETUP {
            return Doing::Setting;
        }
        if t >= self.off - PACK {
            return Doing::Packing;
        }
        let (on, talk) = match self.act {
            Act::Prophet => (14.0, 6.0),
            Act::Theremin => (40.0, 6.0),
            Act::Statue | Act::Club | Act::Dancers => return Doing::Performing,
            Act::Juggler => (24.0, 5.0),
            Act::Spotter => (10.0, 8.0),
            Act::Newsboy => (6.0, 4.0),
            Act::Stargazer => (12.0, 9.0),
            Act::Trader => (15.0, 8.0),
        };
        let period = on + talk;
        let x = (t - self.on + k as f64 * 3.0).rem_euclid(period);
        if x < on {
            Doing::Performing
        } else {
            Doing::Talking
        }
    }

    fn part_at(
        &self,
        life: &StreetLife,
        citizens: &Citizens,
        p: &Part,
        at: DVec2,
        face: DVec2,
        t: f64,
    ) -> Option<StreetPerson> {
        let carrying = matches!(p.role, Role::Performer(..)) && self.act != Act::Club;
        if t < p.arrive {
            return None;
        }
        let there = p.there();
        if t < there {
            let e = p.walk_in.as_ref()?;
            let x = e.len() - (t - p.arrive) * p.pace;
            let (q, d) = citizens.escape_at(e, x);
            let q2 = DVec2::new(q.x, q.z);
            return Some(StreetPerson {
                id: p.id,
                role: p.role,
                pos: DVec3::new(q.x, life.ground_at(q2), q.z),
                facing: (-d).normalize_or(face),
                speed: p.pace,
                doing: Doing::Walking,
                carrying,
            });
        }
        if t < p.leave {
            return Some(StreetPerson {
                id: p.id,
                role: p.role,
                pos: DVec3::new(at.x, life.ground_at(at), at.y),
                facing: face,
                speed: 0.0,
                doing: Doing::Watching,
                carrying: false,
            });
        }
        let e = p.walk_out.as_ref()?;
        let x = (t - p.leave) * p.pace;
        if x > e.len() {
            return None;
        }
        let (q, d) = citizens.escape_at(e, x);
        let q2 = DVec2::new(q.x, q.z);
        Some(StreetPerson {
            id: p.id,
            role: p.role,
            pos: DVec3::new(q.x, life.ground_at(q2), q.z),
            facing: d.normalize_or(face),
            speed: p.pace,
            doing: Doing::Walking,
            carrying,
        })
    }

    fn guest_at(
        &self,
        life: &StreetLife,
        citizens: &Citizens,
        g: &Guest,
        t: f64,
    ) -> Option<StreetPerson> {
        let spots = &self.site.spots;
        let door = self.site.door.unwrap_or(self.site.at);
        let person = |p: DVec2, facing: DVec2, speed: f64, doing: Doing| StreetPerson {
            id: g.id,
            role: Role::Guest(g.look),
            pos: DVec3::new(p.x, life.ground_at(p), p.y),
            facing: facing.normalize_or(DVec2::Y),
            speed,
            doing,
            carrying: false,
        };
        if t < g.arrive || t >= g.enters + IN_DOOR {
            return None;
        }
        if t < g.joins {
            let e = g.walk_in.as_ref()?;
            let x = e.len() - (t - g.arrive) * WATCHER_PACE;
            let (q, d) = citizens.escape_at(e, x);
            let q2 = DVec2::new(q.x, q.z);
            // The last steps from the back of the queue up to their place.
            return Some(person(q2, -d, WATCHER_PACE, Doing::Walking));
        }
        if t >= g.enters {
            // From the head of the queue in through the door.
            let k = ((t - g.enters) / IN_DOOR).clamp(0.0, 1.0);
            let from = spots[0].0;
            let to = door - self.site.face * 0.4;
            return Some(person(from.lerp(to, k), to - from, 1.1, Doing::Walking));
        }
        // In the queue: in place `ahead`, moving up as those ahead go in.
        // Those ahead: everyone who came before and is not in yet (they
        // come and go in in order).
        let me = self.guests.iter().position(|o| o.id == g.id).unwrap_or(0);
        let ahead_of = |time: f64| {
            self.guests[..me]
                .iter()
                .filter(|o| o.enters > time)
                .count()
                .min(spots.len() - 1)
        };
        let now = ahead_of(t);
        let before = ahead_of(t - SHUFFLE);
        if t - g.joins < SHUFFLE {
            // Walking up from the back to their place.
            let k = ((t - g.joins) / SHUFFLE).clamp(0.0, 1.0);
            let (from, to) = (spots[spots.len() - 1].0, spots[now].0);
            if from.distance(to) > 0.05 {
                return Some(person(from.lerp(to, k), to - from, 1.0, Doing::Walking));
            }
        }
        if before > now {
            // Moving up a place.
            let last = self
                .guests
                .iter()
                .filter(|o| o.enters <= t && o.enters > t - SHUFFLE)
                .map(|o| o.enters)
                .fold(t - SHUFFLE, f64::max);
            let k = ((t - last) / SHUFFLE).clamp(0.0, 1.0);
            let (a, b) = (spots[before].0, spots[now].0);
            if k < 1.0 {
                return Some(person(a.lerp(b, k), b - a, 0.8, Doing::Walking));
            }
        }
        let (p, f) = spots[now];
        Some(person(p, f, 0.0, Doing::Queueing))
    }
}

pub mod crime;

#[cfg(test)]
mod tests;
