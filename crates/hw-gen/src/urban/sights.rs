//! Sights: what the world keeps for those who go and look.
//!
//! * **Aether springs** out in the country: terraces of pale stone with a
//!   pool of light on top, crystals standing round it. The nearest lie a
//!   short flight from the town's edge, the best ones far out on the
//!   tablelands.
//! * **Waystones**: old rings of standing stones, tucked away where the
//!   land hides them (a hollow, a canyon floor, the foot of a cliff).
//! * **Wisps**: motes of Aether that gather where few people go: over the
//!   highest roofs, on top of the landmarks, on a mesa's rim, in a grotto
//!   of the Deeps.
//! * **Courses**: flight races through rings down the avenues and over the
//!   country, and rooftop courses from roof to roof.
//!
//! All of it is planned from the world's seed alone, so every machine has
//! the same sights, and each has a number that never changes for a world:
//! a hero's file keeps what they found as bits by those numbers.
//!
//! Two plans, because of who needs them when: the springs and waystones
//! ([`WildSights`]) come from the ground alone and are planned first (the
//! woods leave their ground clear, and collision makes them solid); the
//! rest ([`Sights`]) needs buildings and the Deeps and takes a moment, so
//! it is planned on a worker ([`TownWorld::plan_sights_in_background`]).

use super::footprint::Footprint;
use super::landmarks::LandmarkKind;
use super::world::TownWorld;
use crate::water::segment_distance;
use glam::{DVec2, DVec3};
use hw_core::{det, Rng, Seed};
use std::sync::Arc;

/// The most of each a world has (a hero's file and the wire count on it).
pub const MAX_WISPS: usize = 240;
pub const MAX_SPRINGS: usize = 12;
pub const MAX_STONES: usize = 6;
pub const MAX_COURSES: usize = 8;
/// The most gates of one course.
pub const MAX_GATES: usize = 20;

// ---- Aether springs ----

/// The terraces of a spring, from the ground up: (radius, height of its
/// top over the spring's base). Each step is one a hero walks up.
pub const TERRACES: [(f64, f64); 4] = [(5.2, 0.28), (4.4, 0.56), (3.7, 0.84), (3.0, 1.1)];
/// The pool of light on the top terrace: its radius.
pub const POOL_R: f64 = 2.5;
/// How far the terraces' stone reaches down into the ground (m), so a
/// slope never shows under them.
pub const ROOT: f64 = 3.0;
/// The woods and the grass keep this far from a spring's or a ring's
/// middle (m).
pub const CLEARING: f64 = 13.0;

/// An Aether spring.
#[derive(Clone, Debug, PartialEq)]
pub struct AetherSpring {
    pub pos: DVec2,
    /// The ground at its middle: the terraces stand on this.
    pub base: f64,
    /// The way its tallest crystal stands from the middle (unit).
    pub dir: DVec2,
    /// How far out it lies: 0 near the town, 1 further, 2 the far ones.
    /// The further, the better what it gives.
    pub tier: u8,
    pub seed: Seed,
}

/// A crystal by a spring, in the spring's own frame (x right of `dir`, z
/// along it): where its foot is, its radius there, how tall it is and how
/// far its tip leans from its foot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crystal {
    pub at: DVec2,
    pub r: f64,
    pub height: f64,
    pub lean: DVec2,
    /// The height of the ground it stands on over the spring's base.
    pub foot: f64,
}

impl AetherSpring {
    /// The top of the pool: where a hero stands in the light.
    pub fn pool(&self) -> DVec3 {
        DVec3::new(self.pos.x, self.base + TERRACES[3].1, self.pos.y)
    }

    /// The spring's own frame to the plan.
    pub fn to_plan(&self, l: DVec2) -> DVec2 {
        self.pos + self.dir.perp() * -l.x + self.dir * l.y
    }

    /// Its crystals: a tall one, and smaller ones round the terraces.
    pub fn crystals(&self) -> Vec<Crystal> {
        let mut rng = self.seed.child("crystals").rng();
        let n = 5 + rng.index(3);
        let mut out = Vec::with_capacity(n);
        for k in 0..n {
            // Round the back two thirds: the front stays open to walk in.
            let a = (k as f64 + rng.range(-0.25, 0.25)) / n as f64 * 4.4 - 2.2;
            let ring = rng.range(3.7, 4.7);
            let d = DVec2::new(det::sin(a), det::cos(a));
            let tall = if k == n / 2 {
                rng.range(4.2, 5.4)
            } else {
                rng.range(1.3, 3.1)
            };
            let foot = TERRACES
                .iter()
                .rev()
                .find(|t| ring <= t.0)
                .map_or(0.0, |t| t.1);
            out.push(Crystal {
                at: d * ring,
                r: 0.11 * tall + rng.range(0.08, 0.16),
                height: tall,
                lean: d * rng.range(0.05, 0.3) * tall + d.perp() * rng.range(-0.12, 0.12) * tall,
                foot,
            });
        }
        out
    }

    /// Its solid parts: the terraces and the crystals' feet.
    pub fn footprints(&self) -> Vec<Footprint> {
        let mut out: Vec<Footprint> = TERRACES
            .iter()
            .map(|&(r, y)| Footprint::Disc {
                at: self.pos,
                r,
                y0: self.base - ROOT,
                y1: self.base + y,
            })
            .collect();
        for c in self.crystals() {
            out.push(Footprint::Disc {
                at: self.to_plan(c.at + c.lean * 0.25),
                r: c.r,
                y0: self.base,
                y1: self.base + c.foot + c.height * 0.8,
            });
        }
        out
    }

    /// Is a hero with their feet at `feet` standing in the pool?
    pub fn holds(&self, feet: DVec3) -> bool {
        let top = self.base + TERRACES[3].1;
        DVec2::new(feet.x, feet.z).distance(self.pos) <= POOL_R + 0.3
            && feet.y >= top - 0.6
            && feet.y <= top + 2.5
    }
}

// ---- Waystones ----

/// The ring's radius, to the stones' middles (m).
pub const RING_R: f64 = 6.4;
/// The altar stone in the middle: radius and height.
pub const ALTAR: (f64, f64) = (1.15, 0.28);

/// A ring of standing stones.
#[derive(Clone, Debug, PartialEq)]
pub struct Waystones {
    pub pos: DVec2,
    pub base: f64,
    /// The way its gap (the way in) lies from the middle (unit).
    pub dir: DVec2,
    pub name: String,
    pub seed: Seed,
}

/// One standing stone, in the ring's own frame: where it stands, the way
/// its broad face looks (unit), half its width and thickness at the foot,
/// its height, and the ground under it over the ring's base.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Monolith {
    pub at: DVec2,
    pub face: DVec2,
    pub half: DVec2,
    pub height: f64,
    pub foot: f64,
}

impl Waystones {
    pub fn to_plan(&self, l: DVec2) -> DVec2 {
        self.pos + self.dir.perp() * -l.x + self.dir * l.y
    }

    /// The altar's top: where a hero stands to hear the stones.
    pub fn altar(&self) -> DVec3 {
        DVec3::new(self.pos.x, self.base + ALTAR.1, self.pos.y)
    }

    /// Its stones: seven in a ring, a gap towards `dir`. `ground` gives
    /// the ground's height at a point of the plan (each stands on its own).
    pub fn monoliths(&self, ground: impl Fn(DVec2) -> f64) -> Vec<Monolith> {
        let mut rng = self.seed.child("stones").rng();
        let n = 7;
        (0..n)
            .map(|k| {
                // Eight places round the ring, the one towards `dir` left open.
                let a = std::f64::consts::TAU * (k as f64 + 1.0) / (n as f64 + 1.0)
                    + rng.range(-0.05, 0.05);
                let d = DVec2::new(det::sin(a), det::cos(a));
                let at = d * (RING_R + rng.range(-0.3, 0.3));
                let height = rng.range(3.1, 4.7);
                Monolith {
                    at,
                    face: -d,
                    half: DVec2::new(rng.range(0.62, 0.9), rng.range(0.3, 0.42)),
                    height,
                    foot: ground(self.to_plan(at)) - self.base,
                }
            })
            .collect()
    }

    /// Its solid parts: the stones and the altar.
    pub fn footprints(&self, ground: impl Fn(DVec2) -> f64) -> Vec<Footprint> {
        let mut out = vec![Footprint::Disc {
            at: self.pos,
            r: ALTAR.0,
            y0: self.base - ROOT,
            y1: self.base + ALTAR.1,
        }];
        for m in self.monoliths(ground) {
            out.push(Footprint::Box {
                at: self.to_plan(m.at),
                dir: (self.dir.perp() * -m.face.x + self.dir * m.face.y).normalize_or(DVec2::Y),
                half: m.half,
                y0: self.base + m.foot - 1.0,
                y1: self.base + m.foot + m.height,
            });
        }
        out
    }

    /// Is a hero with their feet at `feet` standing on the altar?
    pub fn holds(&self, feet: DVec3) -> bool {
        DVec2::new(feet.x, feet.z).distance(self.pos) <= ALTAR.0 + 0.5
            && feet.y >= self.base - 0.5
            && feet.y <= self.base + 3.0
    }
}

/// The springs and waystones: planned from the ground alone.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WildSights {
    pub springs: Vec<AetherSpring>,
    pub stones: Vec<Waystones>,
}

impl WildSights {
    /// Is `p` on ground the woods and the grass leave clear?
    pub fn clearing(&self, p: DVec2) -> bool {
        self.springs
            .iter()
            .map(|s| s.pos)
            .chain(self.stones.iter().map(|s| s.pos))
            .any(|c| c.distance_squared(p) < CLEARING * CLEARING)
    }

    /// Every solid part of them (`ground`: the ground's height).
    pub fn footprints(&self, ground: impl Fn(DVec2) -> f64) -> Vec<Footprint> {
        let mut out: Vec<Footprint> = self.springs.iter().flat_map(|s| s.footprints()).collect();
        for s in &self.stones {
            out.extend(s.footprints(&ground));
        }
        out
    }
}

// ---- Wisps ----

/// Where a wisp gathers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WispPlace {
    /// Over a roof.
    Roof,
    /// On top of a landmark.
    Landmark,
    /// Out in the country.
    Wild,
    /// In a grotto or a ruin of the deep underground.
    Deep,
}

/// A wisp: a mote of Aether a hero gathers by touching it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wisp {
    pub at: DVec3,
    pub place: WispPlace,
}

/// A hero gathers a wisp from this near (m, from their middle).
pub const WISP_REACH: f64 = 3.2;

// ---- Courses ----

/// What kind of course.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CourseKind {
    /// Rings in the air: fly through them.
    Flight,
    /// Hoops over the roofs: run and jump, no flying.
    Rooftop,
}

/// One gate of a course.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gate {
    /// A flight ring's middle; a rooftop hoop's middle, over its roof.
    pub at: DVec3,
    /// The way the course runs through it (unit, level).
    pub dir: DVec2,
    /// A rooftop hoop counts from this height up (its roof's walls); a
    /// flight ring has its own middle here.
    pub floor: f64,
}

/// A flight ring's radius, and how near its middle counts as through (m).
pub const RING_RADIUS: f64 = 8.0;
pub const RING_REACH: f64 = 9.5;
/// A rooftop hoop's radius, and how near its roof's middle counts (m, in
/// plan): anywhere on or over that roof, up to [`HOOP_ROOM`] above the hoop.
pub const HOOP_RADIUS: f64 = 3.2;
pub const HOOP_REACH: f64 = 7.0;
pub const HOOP_ROOM: f64 = 14.0;

/// A course: its gates in order (the first is the start, the last the
/// finish) and the times for gold, silver and bronze (s).
#[derive(Clone, Debug, PartialEq)]
pub struct Course {
    pub kind: CourseKind,
    pub name: String,
    pub gates: Vec<Gate>,
    pub pars: [f64; 3],
}

impl Course {
    /// The way from the first gate to the last (m).
    pub fn length(&self) -> f64 {
        self.gates
            .windows(2)
            .map(|w| w[0].at.distance(w[1].at))
            .sum()
    }

    /// Is a hero whose middle is at `p` through gate `k`?
    pub fn through(&self, k: usize, p: DVec3) -> bool {
        let Some(g) = self.gates.get(k) else {
            return false;
        };
        match self.kind {
            CourseKind::Flight => p.distance(g.at) <= RING_REACH,
            CourseKind::Rooftop => {
                DVec2::new(p.x - g.at.x, p.z - g.at.z).length() <= HOOP_REACH
                    && p.y >= g.floor - 1.0
                    && p.y <= g.at.y + HOOP_ROOM
            }
        }
    }

    /// The medal a run of `seconds` earns: 3 gold, 2 silver, 1 bronze, 0 none.
    pub fn medal(&self, seconds: f64) -> u8 {
        self.pars
            .iter()
            .position(|par| seconds <= *par)
            .map_or(0, |k| 3 - k as u8)
    }
}

/// Everything there is to find.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sights {
    pub wild: Arc<WildSights>,
    pub wisps: Vec<Wisp>,
    pub courses: Vec<Course>,
}

// ---- Planning ----

/// How far the town's own built ground reaches from the middle of its land.
fn built_radius(w: &TownWorld) -> f64 {
    w.town
        .pieces
        .iter()
        .flat_map(|p| p.face.pts.iter())
        .map(|p| p.length())
        .fold(200.0, f64::max)
}

/// The difference between the highest and the lowest ground on a circle
/// of radius `r` round `p` and at `p` itself.
fn relief(w: &TownWorld, p: DVec2, r: f64, n: usize) -> (f64, f64, f64) {
    let h = w.ground(p);
    let (mut lo, mut hi) = (h, h);
    for k in 0..n {
        let g = w.ground(p + det::dir2(std::f64::consts::TAU * k as f64 / n as f64) * r);
        lo = lo.min(g);
        hi = hi.max(g);
    }
    (h, lo, hi)
}

/// Is the ground round `p` open country where a thing of radius `r` can
/// stand: level, dry, on no town ground, clear of roads, landmarks, the
/// harbour and the stairs down? Its height if so.
fn open_site(w: &TownWorld, p: DVec2, r: f64) -> Option<f64> {
    let half = w.half();
    // Without a planet the land ends at its edge.
    if w.planet().is_none() && p.abs().max_element() > half - r - 30.0 {
        return None;
    }
    let (h, lo, hi) = relief(w, p, r, 10);
    if hi - lo > 0.11 * r + 0.25 {
        return None;
    }
    if lo - w.water_level(p) < 1.5 {
        return None;
    }
    let (_, lo2, hi2) = relief(w, p, r + 9.0, 10);
    if lo2 - w.water_level(p) < 0.6 || hi2 - lo2 > 9.0 {
        return None;
    }
    let far = r + 16.0;
    if w.in_a_piece(p) || (0..8).any(|k| w.in_a_piece(p + det::dir2(0.785 * k as f64) * far)) {
        return None;
    }
    for road in &w.main_roads {
        let clear = far + road.half + road.pavement + 14.0;
        if road
            .pts
            .windows(2)
            .any(|s| segment_distance(p, s[0], s[1]) < clear)
        {
            return None;
        }
    }
    if w.landmarks.iter().any(|l| l.covers(p, far + 110.0)) {
        return None;
    }
    if w.harbour.as_ref().is_some_and(|h| h.covers(p, far + 60.0)) {
        return None;
    }
    if w.town.graveyards.iter().any(|g| g.at.distance(p) < 160.0) {
        return None;
    }
    Some(h)
}

/// Is a stair down (a way up from the tube, out in the country) within
/// `r` of `p`? Asked of the few sites picked, not of every candidate: it
/// plans the deep underground there.
fn stairs_near(w: &TownWorld, p: DVec2, r: f64) -> bool {
    let m = DVec2::splat(r + 28.0);
    !w.street_holes(p - m, p + m).is_empty()
}

const STONE_NAMES_A: [&str; 10] = [
    "Whispering",
    "Sleeping",
    "Humming",
    "Leaning",
    "Patient",
    "Listening",
    "Moonlit",
    "Elder",
    "Wandering",
    "Silent",
];
const STONE_NAMES_B: [&str; 6] = [
    "Stones",
    "Sisters",
    "Watchers",
    "Ring",
    "Sentinels",
    "Elders",
];

impl WildSights {
    /// The springs and waystones of `w`.
    pub fn plan(w: &TownWorld) -> WildSights {
        let seed = w.seed.child("sights");
        let built = built_radius(w);
        let planet = w.planet().is_some();
        // Bands out from the town's edge: (from, to, how many springs).
        let bands: &[(f64, f64, usize)] = if planet {
            &[(120.0, 800.0, 2), (800.0, 2300.0, 3), (2300.0, 4800.0, 3)]
        } else {
            &[(0.0, 500.0, 2), (0.0, 500.0, 0), (0.0, 500.0, 0)]
        };
        let mut taken: Vec<DVec2> = Vec::new();
        let apart = if planet { 520.0 } else { 160.0 };
        // A candidate in a band: a point, its ground, and the lie of the
        // land round it (how high it stands over what is round it).
        let candidates = |rng: &mut Rng, from: f64, to: f64, n: usize, r: f64| {
            let mut out = Vec::new();
            for _ in 0..n {
                let a = rng.range(0.0, std::f64::consts::TAU);
                let d = if planet {
                    built + rng.range(from, to)
                } else {
                    // On a town's own land: in the fields between its
                    // streets and the land's edge.
                    rng.range(built * 0.55, w.half() - 40.0)
                };
                let p = det::dir2(a) * d;
                let Some(h) = open_site(w, p, r) else {
                    continue;
                };
                let (_, lo, hi) = relief(w, p, 70.0, 8);
                out.push((p, h, h - lo, hi - h));
            }
            out
        };
        let mut springs = Vec::new();
        for (tier, &(from, to, n)) in bands.iter().enumerate() {
            let mut rng = seed.child("springs").child(tier as u64).rng();
            let mut c = candidates(&mut rng, from, to, 260, TERRACES[0].0 + 1.0);
            // Springs rise where the land stands proud: a table's rim, a
            // knoll. The far ones on the highest ground there is.
            c.sort_by(|a, b| {
                let score = |x: &(DVec2, f64, f64, f64)| x.2 - x.3 + 0.02 * x.1 * tier as f64;
                score(b).total_cmp(&score(a))
            });
            let mut got = 0;
            for (p, h, ..) in c {
                if got == n {
                    break;
                }
                if taken.iter().any(|t| t.distance(p) < apart) || stairs_near(w, p, TERRACES[0].0) {
                    continue;
                }
                taken.push(p);
                got += 1;
                let s = seed.child("spring").child(springs.len() as u64);
                let a = s.child("dir").unit() * std::f64::consts::TAU;
                springs.push(AetherSpring {
                    pos: p,
                    base: h,
                    dir: det::dir2(a),
                    tier: tier as u8,
                    seed: s,
                });
            }
        }
        springs.truncate(MAX_SPRINGS);
        // Waystones hide: in a hollow, on a canyon's floor, under a cliff.
        let mut stones = Vec::new();
        let want = if planet { 4 } else { 1 };
        let mut rng = seed.child("stones").rng();
        let (from, to) = if planet {
            (350.0, 3600.0)
        } else {
            (0.0, 500.0)
        };
        let mut c = candidates(&mut rng, from, to, 420, RING_R + 2.0);
        c.sort_by(|a, b| (b.3 - b.2).total_cmp(&(a.3 - a.2)));
        for (p, h, ..) in c {
            if stones.len() == want {
                break;
            }
            if taken.iter().any(|t| t.distance(p) < apart * 1.4) || stairs_near(w, p, RING_R) {
                continue;
            }
            taken.push(p);
            let s = seed.child("ring").child(stones.len() as u64);
            let mut r = s.rng();
            let name = loop {
                let name = format!("The {} {}", r.pick(&STONE_NAMES_A), r.pick(&STONE_NAMES_B));
                if stones.iter().all(|x: &Waystones| x.name != name) {
                    break name;
                }
            };
            stones.push(Waystones {
                pos: p,
                base: h,
                dir: det::dir2(r.range(0.0, std::f64::consts::TAU)),
                name,
                seed: s,
            });
        }
        stones.truncate(MAX_STONES);
        WildSights { springs, stones }
    }
}

const FLIGHT_NAMES_A: [&str; 8] = [
    "Comet",
    "Rocket",
    "Meteor",
    "Starling",
    "Aether",
    "Thunderbolt",
    "Silver Arrow",
    "Skylark",
];
const FLIGHT_NAMES_B: [&str; 5] = ["Run", "Dash", "Derby", "Cup", "Sprint"];
const ROOF_NAMES_A: [&str; 8] = [
    "Chimney",
    "Skyline",
    "Cornice",
    "Pigeon",
    "Gargoyle",
    "Water Tower",
    "Fire Escape",
    "Weathercock",
];
const ROOF_NAMES_B: [&str; 5] = ["Hop", "Scramble", "Steeplechase", "Caper", "Gallop"];

/// Speeds the medal times go by (m/s): gold, silver, bronze.
const FLIGHT_PACE: [f64; 3] = [19.0, 14.5, 10.0];
const ROOF_PACE: [f64; 3] = [11.5, 8.5, 5.5];

fn pars(length: f64, gates: usize, pace: [f64; 3]) -> [f64; 3] {
    // A moment per gate for the turn into it.
    pace.map(|v| ((length / v + 0.35 * gates as f64) * 2.0).round() / 2.0)
}

fn name_of(rng: &mut Rng, a: &[&str], b: &[&str], have: &[Course]) -> String {
    loop {
        let name = format!("The {} {}", rng.pick(a), rng.pick(b));
        if have.iter().all(|c| c.name != name) {
            return name;
        }
    }
}

/// A point `d` metres along a line of points, and the way the line runs there.
fn along(pts: &[DVec2], d: f64) -> (DVec2, DVec2) {
    let mut left = d;
    for s in pts.windows(2) {
        let l = s[0].distance(s[1]);
        if left <= l && l > 1e-9 {
            return (s[0].lerp(s[1], left / l), (s[1] - s[0]) / l);
        }
        left -= l;
    }
    let n = pts.len();
    (pts[n - 1], (pts[n - 1] - pts[n - 2]).normalize_or(DVec2::Y))
}

impl Sights {
    /// Everything there is to find in `w`. Slow (it plans a few dozen
    /// districts' buildings and some tiles of the Deeps): on a worker.
    pub fn plan(w: &TownWorld) -> Sights {
        let wild = w.wild_sights().clone();
        let seed = w.seed.child("sights");
        let built = built_radius(w);
        let planet = w.planet().is_some();
        let area_km2 = (built * 2.0).powi(2) / 1e6;
        let total = ((36.0 + 16.0 * area_km2) as usize).clamp(36, MAX_WISPS);
        let mut wisps: Vec<Wisp> = Vec::with_capacity(total);

        // On top of the landmarks.
        for l in &w.landmarks {
            if l.height < 14.0 || l.kind == LandmarkKind::Hospital {
                continue;
            }
            wisps.push(Wisp {
                at: DVec3::new(l.pos.x, l.base + l.height + 2.4, l.pos.y),
                place: WispPlace::Landmark,
            });
        }
        wisps.truncate(total / 5);

        // Over the roofs: the highest of a neighbourhood, and plain ones
        // a jump reaches.
        let hoods = neighbourhoods(w, seed);
        let roofs = (total * 45 / 100).max(8);
        let each = roofs.div_ceil(hoods.len().max(1));
        let mut rng = seed.child("roof-wisps").rng();
        for hood in &hoods {
            let mut order: Vec<usize> = (0..hood.len()).collect();
            rng.shuffle(&mut order);
            // The highest first, then whichever.
            if let Some(high) =
                (0..hood.len()).max_by(|&a, &b| hood[a].top.total_cmp(&hood[b].top).then(b.cmp(&a)))
            {
                order.retain(|&i| i != high);
                order.insert(0, high);
            }
            let mut used: Vec<DVec2> = Vec::new();
            for i in order {
                let r = hood[i];
                if used.len() == each {
                    break;
                }
                if r.top - r.ground < 7.0 || used.iter().any(|u| u.distance(r.mid) < 60.0) {
                    continue;
                }
                used.push(r.mid);
                wisps.push(Wisp {
                    at: DVec3::new(r.mid.x, r.top + 1.9, r.mid.y),
                    place: WispPlace::Roof,
                });
            }
        }

        // In the Deeps' grottos and the buried ruins.
        let deep = total * 8 / 100;
        let deep_end = wisps.len() + deep;
        let half = w.half();
        let n = (half / super::deep::TILE).floor() as i64;
        let mut tiles: Vec<(i64, i64)> =
            (-n..n).flat_map(|x| (-n..n).map(move |y| (x, y))).collect();
        seed.child("deep-wisps").rng().shuffle(&mut tiles);
        for k in tiles.into_iter().take(deep * 2 + 6) {
            if wisps.len() >= deep_end {
                break;
            }
            let key = super::deep::tile_of(DVec2::new(
                (k.0 as f64 + 0.5) * super::deep::TILE,
                (k.1 as f64 + 0.5) * super::deep::TILE,
            ));
            if !w.deep_in_land(key) {
                continue;
            }
            let t = w.deep_tile(key);
            if let Some(g) = t.grottos.first() {
                wisps.push(Wisp {
                    at: DVec3::new(g.centre.x, g.floor + 2.3, g.centre.y),
                    place: WispPlace::Deep,
                });
            } else if let Some(r) = t.ruins.first() {
                if let Some(v) = t.voids.get(r.rotunda) {
                    let c = v.world((v.min + v.max) * 0.5);
                    wisps.push(Wisp {
                        at: DVec3::new(c.x, v.floor_at(c) + 2.3, c.y),
                        place: WispPlace::Deep,
                    });
                }
            }
        }

        // Out in the country: where the land stands proud or falls away.
        let mut rng = seed.child("wild-wisps").rng();
        let reach = if planet { 3200.0 } else { 0.0 };
        let mut c: Vec<(f64, DVec3)> = Vec::new();
        for _ in 0..(total.saturating_sub(wisps.len()) * 14) {
            let a = rng.range(0.0, std::f64::consts::TAU);
            let d = if planet {
                built * 0.9 + rng.range(0.0, reach)
            } else {
                rng.range(built * 0.5, half - 30.0)
            };
            let p = det::dir2(a) * d;
            if w.in_a_piece(p) || wild.clearing(p) {
                continue;
            }
            let (h, lo, hi) = relief(w, p, 24.0, 6);
            if h - w.water_level(p) < 1.0 {
                continue;
            }
            // A rim: the ground falls away on one side and not the other.
            c.push(((h - lo) - (hi - h) * 2.0, DVec3::new(p.x, h + 1.5, p.y)));
        }
        c.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut wild_at: Vec<DVec3> = Vec::new();
        for (_, at) in c {
            if wisps.len() >= total {
                break;
            }
            if wild_at.iter().any(|o| o.distance(at) < 170.0) {
                continue;
            }
            wild_at.push(at);
            wisps.push(Wisp {
                at,
                place: WispPlace::Wild,
            });
        }
        wisps.truncate(MAX_WISPS);

        let mut courses = Vec::new();
        Self::flight_courses(w, seed, built, &mut courses);
        Self::rooftop_courses(seed, built, &hoods, &mut courses);
        courses.truncate(MAX_COURSES);
        Sights {
            wild,
            wisps,
            courses,
        }
    }

    /// Races down the longest avenues, and one over the country.
    fn flight_courses(w: &TownWorld, seed: Seed, built: f64, out: &mut Vec<Course>) {
        let mut rng = seed.child("flight").rng();
        let length = |r: &super::world::WorldRoad| -> f64 {
            r.pts.windows(2).map(|s| s[0].distance(s[1])).sum()
        };
        // The longest main roads through the town's own ground.
        let mut roads: Vec<(f64, usize)> = w
            .main_roads
            .iter()
            .enumerate()
            .map(|(i, r)| (length(r), i))
            .filter(|(l, _)| *l > 700.0)
            .collect();
        roads.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        let avenues = if built > 1500.0 { 2 } else { 1 };
        let mut starts: Vec<DVec2> = Vec::new();
        for &(len, i) in roads.iter() {
            if out.len() >= avenues {
                break;
            }
            let road = &w.main_roads[i];
            let run = len.min(2600.0);
            let from = (len - run) * 0.5;
            let step = (run / 13.0).clamp(110.0, 200.0);
            let n = ((run / step) as usize + 1).min(MAX_GATES);
            let (start, _) = along(&road.pts, from);
            if starts.iter().any(|s| s.distance(start) < 500.0) {
                continue;
            }
            let wave = rng.range(0.0, 6.0);
            let mut gates = Vec::with_capacity(n);
            for k in 0..n {
                let (p, dir) = along(&road.pts, from + step * k as f64);
                // Up and down the avenue like a swallow.
                let lift = 31.0 + 11.0 * det::sin(wave + k as f64 * 1.1);
                // Over the roofs where a landmark's plinth takes the road.
                let y = w.road_height(p) + lift;
                gates.push(Gate {
                    at: DVec3::new(p.x, y, p.y),
                    dir,
                    floor: y,
                });
            }
            if gates.len() < 5 {
                continue;
            }
            starts.push(start);
            let mut c = Course {
                kind: CourseKind::Flight,
                name: name_of(&mut rng, &FLIGHT_NAMES_A, &FLIGHT_NAMES_B, out),
                gates,
                pars: [0.0; 3],
            };
            c.pars = pars(c.length(), c.gates.len(), FLIGHT_PACE);
            out.push(c);
        }
        // Over the country: an arc round the town, low through the
        // valleys and up over the tables' rims.
        if w.planet().is_some() {
            let a0 = rng.range(0.0, std::f64::consts::TAU);
            let turn = if rng.chance(0.5) { 1.0 } else { -1.0 };
            let n = 14;
            // A gate every 210 m or so along the arc.
            let step = 210.0 / (built + 400.0);
            let mut gates: Vec<Gate> = Vec::with_capacity(n);
            for k in 0..n {
                let a = a0 + turn * k as f64 * step;
                // Of a few distances out, the one with the lowest ground
                // on even gates and the highest on odd ones.
                let mut best: Option<(f64, DVec2)> = None;
                for j in 0..5 {
                    let p = det::dir2(a) * (built + 280.0 + 60.0 * j as f64);
                    if w.in_a_piece(p) || w.landmarks.iter().any(|l| l.covers(p, 220.0)) {
                        continue;
                    }
                    let g = w.ground(p).max(w.water_level(p));
                    let score = if k % 2 == 0 { -g } else { g };
                    if best.is_none_or(|b| score > b.0) {
                        best = Some((score, p));
                    }
                }
                let Some((_, p)) = best else {
                    continue;
                };
                let (_, _, hi) = relief(w, p, 20.0, 6);
                let y = hi.max(w.water_level(p)) + 22.0;
                gates.push(Gate {
                    at: DVec3::new(p.x, y, p.y),
                    dir: DVec2::Y,
                    floor: y,
                });
            }
            for k in 0..gates.len() {
                let (a, b) = (
                    gates[k.saturating_sub(1)].at,
                    gates[(k + 1).min(gates.len() - 1)].at,
                );
                gates[k].dir = DVec2::new(b.x - a.x, b.z - a.z).normalize_or(DVec2::Y);
            }
            if gates.len() >= 8 {
                let mut c = Course {
                    kind: CourseKind::Flight,
                    name: name_of(
                        &mut rng,
                        &["Mesa", "Canyon", "Tableland"],
                        &FLIGHT_NAMES_B,
                        out,
                    ),
                    gates,
                    pars: [0.0; 3],
                };
                c.pars = pars(c.length(), c.gates.len(), FLIGHT_PACE);
                out.push(c);
            }
        }
    }

    /// Courses from roof to roof, where the houses stand close.
    fn rooftop_courses(seed: Seed, built: f64, hoods: &[Vec<RoofTop>], out: &mut Vec<Course>) {
        let mut rng = seed.child("rooftop").rng();
        let want = if built > 1500.0 { 2 } else { 1 };
        let mut made = 0;
        // Where the most roofs stand high enough to run on.
        let fit = |r: &RoofTop| r.walls - r.ground >= 6.0 && r.size >= 9.0;
        let mut order: Vec<usize> = (0..hoods.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(hoods[i].iter().filter(|r| fit(r)).count()));
        for h in order {
            if made == want {
                break;
            }
            let roofs: Vec<RoofTop> = hoods[h].iter().copied().filter(fit).collect();
            if roofs.len() < 10 {
                continue;
            }
            // From a roof at the neighbourhood's edge across it: on to the
            // next roof that lies ahead, a good jump away and not much higher.
            let a = rng.range(0.0, std::f64::consts::TAU);
            let mut heading = det::dir2(a);
            let Some(first) = (0..roofs.len()).min_by(|&x, &y| {
                roofs[x]
                    .mid
                    .dot(heading)
                    .total_cmp(&roofs[y].mid.dot(heading))
                    .then(x.cmp(&y))
            }) else {
                continue;
            };
            let mut path = vec![first];
            while path.len() < 12 {
                let here = roofs[*path.last().expect("a roof")];
                let next = (0..roofs.len())
                    .filter(|i| !path.contains(i))
                    .filter_map(|i| {
                        let r = roofs[i];
                        let d = r.mid - here.mid;
                        let l = d.length();
                        let ahead = d.dot(heading) / l.max(1e-9);
                        (l > 26.0
                            && l < 78.0
                            && ahead > 0.25
                            && r.top - here.walls < 11.0
                            && here.walls - r.walls < 30.0)
                            .then_some((i, (l - 46.0).abs() * 0.02 - ahead))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
                let Some((i, _)) = next else {
                    break;
                };
                let d = (roofs[i].mid - here.mid).normalize_or(heading);
                // The course swings round slowly: a hook, not a line.
                heading = (heading * 0.6 + d * 0.4 + heading.perp() * 0.16).normalize_or(d);
                path.push(i);
            }
            if path.len() < 7 {
                continue;
            }
            let mut gates: Vec<Gate> = path
                .iter()
                .map(|&i| {
                    let r = roofs[i];
                    Gate {
                        at: DVec3::new(r.mid.x, r.top + 3.0, r.mid.y),
                        dir: DVec2::Y,
                        floor: r.walls,
                    }
                })
                .collect();
            for k in 0..gates.len() {
                let (a, b) = (
                    gates[k.saturating_sub(1)].at,
                    gates[(k + 1).min(gates.len() - 1)].at,
                );
                gates[k].dir = DVec2::new(b.x - a.x, b.z - a.z).normalize_or(DVec2::Y);
            }
            made += 1;
            let mut course = Course {
                kind: CourseKind::Rooftop,
                name: name_of(&mut rng, &ROOF_NAMES_A, &ROOF_NAMES_B, out),
                gates,
                pars: [0.0; 3],
            };
            course.pars = pars(course.length(), course.gates.len(), ROOF_PACE);
            out.push(course);
        }
    }
}

/// A roof: its middle, the ground it stands on, the height of its walls
/// and of its highest point, and its narrower side.
#[derive(Clone, Copy, Debug)]
struct RoofTop {
    mid: DVec2,
    ground: f64,
    walls: f64,
    top: f64,
    size: f64,
}

/// How many neighbourhoods' roofs the sights are planned from, and the
/// side of each (m). Planning a district's buildings takes a moment, so
/// it is a few, spread over the town.
const HOODS: usize = 14;
const HOOD: f64 = 340.0;

/// The roofs of a few neighbourhoods spread over the town.
fn neighbourhoods(w: &TownWorld, seed: Seed) -> Vec<Vec<RoofTop>> {
    let mut pieces: Vec<usize> = (0..w.town.pieces.len()).collect();
    seed.child("hoods").rng().shuffle(&mut pieces);
    let mut at: Vec<DVec2> = Vec::new();
    let mut out = Vec::new();
    for i in pieces {
        if out.len() == HOODS {
            break;
        }
        let Some(p) = super::inner_point(&w.town.pieces[i].face.pts) else {
            continue;
        };
        if at.iter().any(|q| q.distance(p) < HOOD * 1.2) {
            continue;
        }
        let m = DVec2::splat(HOOD * 0.5);
        let roofs: Vec<RoofTop> = w
            .skyline(p - m, p + m)
            .into_iter()
            .map(|(_, b)| RoofTop {
                mid: b.site.origin
                    + (b.site.along * b.site.size.x + b.site.inward * b.site.size.y) * 0.5,
                ground: b.base,
                walls: b.base + b.site.height(),
                top: b.base + b.site.top(),
                size: b.site.size.min_element(),
            })
            .collect();
        if roofs.len() < 4 {
            continue;
        }
        at.push(p);
        out.push(roofs);
    }
    out
}

impl TownWorld {
    /// The springs and waystones (planned on first use, from the ground).
    pub fn wild_sights(&self) -> &Arc<WildSights> {
        self.wild_sights
            .get_or_init(|| Arc::new(WildSights::plan(self)))
    }

    /// Everything there is to find, planned now if it is not yet: slow,
    /// for a worker and for tests.
    pub fn sights(&self) -> &Arc<Sights> {
        self.sights.get_or_init(|| Arc::new(Sights::plan(self)))
    }

    /// Everything there is to find, once it is planned.
    pub fn sights_ready(&self) -> Option<&Arc<Sights>> {
        self.sights.get()
    }

    /// Plans the sights on a thread of their own, for
    /// [`Self::sights_ready`]. Whoever asks [`Self::sights`] meanwhile
    /// waits for this one plan: there is never a second.
    pub fn plan_sights_in_background(self: &Arc<Self>) {
        if self.sights.get().is_some() {
            return;
        }
        let me = self.clone();
        let _ = std::thread::Builder::new()
            .name("sights".into())
            .spawn(move || {
                me.sights();
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planet::{choose_home, PlanetMap, PlanetPatch};
    use crate::urban::TownParams;

    fn town(seed: u64, people: f64) -> TownWorld {
        TownWorld::generate(Seed(seed), TownParams::sample(Seed(seed), people))
    }

    fn town_on_planet(seed: u64, people: f64) -> TownWorld {
        let map = PlanetMap::build(Seed(seed), 192);
        let patch = Arc::new(PlanetPatch::new(&map, choose_home(&map)));
        let params = TownParams::on_planet(Seed(seed), people, &patch);
        TownWorld::generate_on_or_stop(Seed(seed), params, Some(patch), &hw_core::Cancel::new())
            .expect("not cancelled")
    }

    fn check(w: &TownWorld) {
        let t = std::time::Instant::now();
        let s = w.sights().clone();
        eprintln!(
            "sights in {:.2} s: {} springs, {} stones, {} wisps ({} roof, {} landmark, {} deep, {} wild), {} courses",
            t.elapsed().as_secs_f64(),
            s.wild.springs.len(),
            s.wild.stones.len(),
            s.wisps.len(),
            s.wisps.iter().filter(|x| x.place == WispPlace::Roof).count(),
            s.wisps.iter().filter(|x| x.place == WispPlace::Landmark).count(),
            s.wisps.iter().filter(|x| x.place == WispPlace::Deep).count(),
            s.wisps.iter().filter(|x| x.place == WispPlace::Wild).count(),
            s.courses.len(),
        );
        for c in &s.courses {
            eprintln!(
                "  {:?} {}: {} gates, {:.0} m, pars {:?}",
                c.kind,
                c.name,
                c.gates.len(),
                c.length(),
                c.pars
            );
        }
        for sp in &s.wild.springs {
            eprintln!(
                "  spring tier {} at {:.0},{:.0} y {:.0}",
                sp.tier, sp.pos.x, sp.pos.y, sp.base
            );
        }
        for st in &s.wild.stones {
            eprintln!(
                "  {} at {:.0},{:.0} y {:.0}",
                st.name, st.pos.x, st.pos.y, st.base
            );
        }
        assert!(!s.wild.springs.is_empty(), "a world has a spring");
        assert!(s.wild.springs.len() <= MAX_SPRINGS && s.wild.stones.len() <= MAX_STONES);
        assert!(s.wisps.len() >= 20 && s.wisps.len() <= MAX_WISPS);
        assert!(!s.courses.is_empty() && s.courses.len() <= MAX_COURSES);
        // Springs and rings stand on dry, open, level ground, clear of
        // each other.
        let spots: Vec<DVec2> = s
            .wild
            .springs
            .iter()
            .map(|x| x.pos)
            .chain(s.wild.stones.iter().map(|x| x.pos))
            .collect();
        for (i, a) in spots.iter().enumerate() {
            assert!(!w.in_a_piece(*a), "on town ground");
            assert!(w.water_at(*a).is_none(), "under water");
            for b in &spots[..i] {
                assert!(a.distance(*b) > 100.0, "two sights on one spot");
            }
        }
        // A wisp hangs in the open: over the ground, or under it in a void.
        for x in &s.wisps {
            let g = w.ground(DVec2::new(x.at.x, x.at.z));
            match x.place {
                WispPlace::Deep => {
                    assert!(x.at.y < g - 5.0, "a deep wisp above ground");
                    assert!(
                        w.deep_open_at(x.at),
                        "a deep wisp in the rock at {:?}",
                        x.at
                    );
                }
                _ => assert!(x.at.y > g + 1.0, "a wisp in the ground at {:?}", x.at),
            }
        }
        for (i, a) in s.wisps.iter().enumerate() {
            for b in &s.wisps[..i] {
                assert!(a.at.distance(b.at) > 20.0, "two wisps on one spot");
            }
        }
        // Courses: gates a fair way apart, medal times in order.
        for c in &s.courses {
            assert!(c.gates.len() >= 5 && c.gates.len() <= MAX_GATES);
            assert!(c.pars[0] < c.pars[1] && c.pars[1] < c.pars[2]);
            for g in c.gates.windows(2) {
                let d = g[0].at.distance(g[1].at);
                assert!(d > 20.0 && d < 420.0, "gates {d:.0} m apart in {}", c.name);
            }
            for g in &c.gates {
                let p = DVec2::new(g.at.x, g.at.z);
                assert!(g.at.y > w.ground(p) + 5.0, "a gate in the ground");
            }
            assert_eq!(c.medal(c.pars[0] - 0.1), 3);
            assert_eq!(c.medal(c.pars[2] + 0.1), 0);
        }
        // The same again from scratch.
        assert_eq!(*s, Sights::plan(w), "sights differ between two plans");
    }

    #[test]
    fn a_town_has_sights_and_they_are_the_same_every_time() {
        check(&town(1, 30_000.0));
    }

    #[test]
    #[ignore = "daily (slow blocker): scripts/check.sh --daily, and CI's first run of the day"]
    fn a_town_on_its_planet_has_sights_far_out() {
        let w = town_on_planet(1, 60_000.0);
        check(&w);
        let s = w.sights();
        assert!(s.wild.springs.iter().any(|x| x.tier == 2), "no far spring");
        assert!(s.wild.stones.len() >= 2, "few waystones");
        assert!(
            s.courses.iter().any(|c| c.kind == CourseKind::Rooftop),
            "no rooftop course"
        );
    }

    #[test]
    fn a_spring_and_a_ring_know_who_stands_in_them() {
        let s = AetherSpring {
            pos: DVec2::new(10.0, 20.0),
            base: 5.0,
            dir: DVec2::Y,
            tier: 0,
            seed: Seed(3),
        };
        assert!(s.holds(s.pool()));
        assert!(!s.holds(s.pool() + DVec3::X * 4.0));
        assert!(!s.holds(s.pool() + DVec3::Y * 6.0));
        // Every crystal stands on the terraces, clear of the pool.
        for c in s.crystals() {
            assert!(c.at.length() - c.r > POOL_R + 0.3 && c.at.length() < TERRACES[0].0);
        }
        let r = Waystones {
            pos: DVec2::ZERO,
            base: 0.0,
            dir: DVec2::X,
            name: "x".into(),
            seed: Seed(4),
        };
        assert!(r.holds(r.altar()));
        let stones = r.monoliths(|_| 0.0);
        assert_eq!(stones.len(), 7);
        // The way in stays open.
        for m in &stones {
            assert!(r.to_plan(m.at).normalize().dot(r.dir) < 0.9);
        }
    }
}
