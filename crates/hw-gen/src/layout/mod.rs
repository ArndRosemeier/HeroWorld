//! Interiors at game scale: the new interior designer (step 1, offices).
//!
//! The old planner ([`crate::building`]) lays out realistic floors: one
//! storey height, rooms off a corridor, a magic lift in a core. Fights
//! indoors need room, so a [`Layout`] is built for play instead:
//!
//! - **Spaces of any height.** A [`Space`] is a box: a floor rectangle, the
//!   height it stands at and how tall it is. A lobby can be two storeys
//!   high, an atrium the whole building, a split-level block half a storey
//!   off its neighbour. Non-nested spaces fill the building's volume
//!   exactly, column by column (see [`Layout::problems`]).
//! - **Things inside tall spaces** are nested spaces ([`Space::within`]):
//!   bridges over an atrium, landings in a stair hall, lift shafts.
//! - **Real stairs and lifts.** A [`Stair`] is a list of flights and landings
//!   inside one tall host space; a [`Lift`] runs in a shaft and stops at
//!   floors. Nobody floats up a glowing core.
//! - **Links** join spaces at a height: doors, wide arches, open edges, rails
//!   along galleries and glass walls that look into a hall.
//!
//! Coordinates are the building's (as in [`crate::building`]): x along the
//! street front, z inwards, y up from the ground floor. Pure data from the
//! seed, like every plan.

pub mod furnish;
pub mod lifts;
pub mod office;
pub mod people;
pub mod solid;
mod stairs;
#[cfg(test)]
mod tests;

use crate::geom::Rect;
use glam::{DVec2, DVec3};

/// Highest riser (m). Game scale: shallow, long stairs read grand and are
/// easy to fight on.
pub const RISE: f64 = 0.2;
/// Tread depth (m).
pub const TREAD: f64 = 0.35;
/// Clear height a person needs over a floor or a stair (m).
pub const HEADROOM: f64 = 2.6;
/// The smallest open-plan hall, either way (m).
pub const MIN_HALL: f64 = 10.0;
/// The narrowest way: galleries, bridges, concourses (m).
pub const MIN_WAY: f64 = 5.0;
/// Door widths (m).
pub const DOOR: f64 = 2.4;
pub const DOUBLE: f64 = 4.0;
pub const ARCH: f64 = 7.0;
pub const ENTRANCE: f64 = 6.0;
/// Room kept clear in front of a doorway, either side (m).
pub const APRON: f64 = 1.6;

const EPS: f64 = 1e-6;

/// What a space is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpaceKind {
    /// The low entrance hall under a gallery.
    Vestibule,
    /// The grand entrance hall, often two storeys high.
    Lobby,
    /// A hall through several storeys, ringed by galleries.
    Atrium,
    /// A walkway along the edge of a tall space, behind a rail.
    Gallery,
    /// The low walk round the foot of an atrium, open to it.
    Arcade,
    /// A walkway across a tall space (nested).
    Bridge,
    /// A floor inside a tall stair space (nested).
    Landing,
    /// A wide hall-like corridor through the building.
    Concourse,
    /// Open-plan offices: a big room with columns.
    Hall,
    /// The top floor's tall room.
    Boardroom,
    /// A shop or cafe on the street.
    Shop,
    /// A tall room for a stair (and often the lifts).
    StairHall,
    /// A lift shaft (nested in the hall it rises through).
    Shaft,
    /// The open slot between split-level blocks, with the stairs.
    Canyon,
    /// Storage and plant rooms in leftover corners.
    Service,
}

/// A box of room: everything at one height on one rectangle.
#[derive(Clone, Debug, PartialEq)]
pub struct Space {
    pub kind: SpaceKind,
    pub rect: Rect,
    /// Floor height above the building's ground.
    pub y: f64,
    /// Clear height up to the next slab (or the roof).
    pub h: f64,
    /// The tall space this one stands inside (bridges, landings, shafts).
    pub within: Option<usize>,
}

impl Space {
    pub fn top(&self) -> f64 {
        self.y + self.h
    }
}

/// How two spaces meet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LinkKind {
    /// A single door.
    Door,
    /// A pair of doors.
    Double,
    /// A wide opening without doors.
    Arch,
    /// No wall at all along the stretch.
    Open,
    /// A balustrade: you see through and can jump over, but don't walk.
    Rail,
    /// A glass wall into a tall hall.
    Glass,
    /// The way in from the street (`b` is None).
    Entrance,
}

impl LinkKind {
    /// Can people walk through it?
    pub fn passable(self) -> bool {
        !matches!(self, Self::Rail | Self::Glass)
    }
}

/// A stretch of shared wall where two spaces meet.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub a: usize,
    /// The other space, or None for the street.
    pub b: Option<usize>,
    pub kind: LinkKind,
    /// The floor height the opening stands on.
    pub y: f64,
    /// Centre of the opening on the wall line (x, z).
    pub at: DVec2,
    /// True if the wall runs along x.
    pub along_x: bool,
    pub width: f64,
}

impl Link {
    /// The opening as a segment on its wall line.
    pub fn ends(&self) -> (DVec2, DVec2) {
        let d = if self.along_x { DVec2::X } else { DVec2::Y } * (self.width * 0.5);
        (self.at - d, self.at + d)
    }

    /// The floor kept clear on both sides of the opening.
    pub fn apron(&self) -> Rect {
        let (a, b) = self.ends();
        let n = if self.along_x { DVec2::Y } else { DVec2::X } * APRON;
        Rect::new(a.min(b) - n, a.max(b) + n)
    }
}

/// The shape of a stair.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StairKind {
    /// One flight, or two in line with a landing between.
    Straight,
    /// Two flights side by side up the walls of a hall.
    Twin,
    /// One wide flight up to a landing, then two back the other way.
    Imperial,
    /// Two curved flights sweeping round to meet at the top.
    Horseshoe,
    /// Up one way to a half landing, back the other way: a stairwell.
    DogLeg,
    /// Round and round an open eye (often with a cage lift in it).
    Spiral,
    /// Half flights between blocks half a storey apart.
    Split,
}

/// One flight of steps.
#[derive(Clone, Debug, PartialEq)]
pub enum Flight {
    /// Straight steps: `from` is the middle of the bottom step's front edge
    /// (y = its foot), climbing along `dir` (a unit axis).
    Straight {
        from: DVec3,
        dir: DVec2,
        width: f64,
        risers: u32,
        rise: f64,
    },
    /// Steps round a centre, from angle `a0` (radians, 0 = +x, pi/2 = +z)
    /// turning by `sweep` (signed).
    Arc {
        centre: DVec2,
        r_in: f64,
        r_out: f64,
        a0: f64,
        sweep: f64,
        y0: f64,
        risers: u32,
        rise: f64,
    },
}

impl Flight {
    pub fn y0(&self) -> f64 {
        match *self {
            Flight::Straight { from, .. } => from.y,
            Flight::Arc { y0, .. } => y0,
        }
    }

    pub fn y1(&self) -> f64 {
        match *self {
            Flight::Straight { risers, rise, .. } | Flight::Arc { risers, rise, .. } => {
                self.y0() + risers as f64 * rise
            }
        }
    }

    /// Plan length of a straight flight: one tread less than its risers
    /// (the top riser steps onto the landing).
    pub fn run(risers: u32) -> f64 {
        (risers.max(1) - 1) as f64 * TREAD
    }

    /// The flight's outline in plan (a bounding box for arcs).
    pub fn footprint(&self) -> Rect {
        match *self {
            Flight::Straight {
                from,
                dir,
                width,
                risers,
                ..
            } => {
                let a = DVec2::new(from.x, from.z);
                let side = dir.perp() * (width * 0.5);
                let b = a + dir * Self::run(risers);
                let (p, q) = (a - side, b + side);
                Rect::new(p.min(q), p.max(q))
            }
            Flight::Arc {
                centre,
                r_in,
                r_out,
                a0,
                sweep,
                ..
            } => {
                let mut lo = DVec2::splat(f64::INFINITY);
                let mut hi = DVec2::splat(f64::NEG_INFINITY);
                let n = 32;
                for i in 0..=n {
                    let a = a0 + sweep * i as f64 / n as f64;
                    let d = DVec2::new(hw_core::det::cos(a), hw_core::det::sin(a));
                    for r in [r_in, r_out] {
                        lo = lo.min(centre + d * r);
                        hi = hi.max(centre + d * r);
                    }
                }
                Rect::new(lo, hi)
            }
        }
    }

    /// Height of the step surface over a plan point inside the flight, if
    /// any (used for headroom checks).
    pub fn height_at(&self, p: DVec2) -> Option<f64> {
        match *self {
            Flight::Straight {
                from,
                dir,
                width,
                risers,
                rise,
            } => {
                let a = DVec2::new(from.x, from.z);
                let s = (p - a).dot(dir);
                let t = (p - a).dot(dir.perp()).abs();
                if t > width * 0.5 || s < 0.0 || s > Self::run(risers) {
                    return None;
                }
                Some(from.y + ((s / TREAD).floor() + 1.0).min(risers as f64) * rise)
            }
            Flight::Arc {
                centre,
                r_in,
                r_out,
                a0,
                sweep,
                y0,
                risers,
                rise,
            } => {
                let d = p - centre;
                let r = d.length();
                if r < r_in || r > r_out {
                    return None;
                }
                let a = hw_core::det::atan2(d.y, d.x);
                let tau = std::f64::consts::TAU;
                let mut u = (a - a0) * sweep.signum();
                u = u.rem_euclid(tau);
                if u > sweep.abs() {
                    return None;
                }
                let k = (u / sweep.abs() * risers as f64).floor() + 1.0;
                Some(y0 + k.min(risers as f64) * rise)
            }
        }
    }
}

/// A floor inside a stair: a half landing, or the step-off at a level.
#[derive(Clone, Debug, PartialEq)]
pub struct Landing {
    pub rect: Rect,
    pub y: f64,
}

/// A stair, inside one tall host space.
#[derive(Clone, Debug, PartialEq)]
pub struct Stair {
    pub kind: StairKind,
    pub host: usize,
    pub flights: Vec<Flight>,
    pub landings: Vec<Landing>,
}

/// What kind of lift.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LiftKind {
    /// A wrought brass cage you can see through, with a folding gate.
    Cage,
    /// A chrome streamline car with a floor dial over the doors.
    Streamline,
    /// A glass capsule riding up the side of an atrium.
    Glass,
    /// A wide open platform for crates.
    Freight,
    /// A chain of open cars that never stops: step on, step off.
    Paternoster,
}

/// A lift in its shaft, stopping at floors.
#[derive(Clone, Debug, PartialEq)]
pub struct Lift {
    pub kind: LiftKind,
    /// Its shaft, a [`SpaceKind::Shaft`].
    pub shaft: usize,
    /// Floor heights it stops at, lowest first.
    pub stops: Vec<f64>,
}

/// A free-standing column: cover in a fight.
#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    pub at: DVec2,
    pub r: f64,
    pub y: f64,
    pub h: f64,
}

/// How the building is organised.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scheme {
    /// A grand lobby two storeys high at the front, a wide concourse back
    /// to a stair hall, open-plan halls either side.
    GrandHall,
    /// An atrium through every storey, galleries on three sides, bridges
    /// across it and glass lifts.
    Atrium,
    /// Front and back blocks half a storey apart, joined by half flights
    /// across a tall canyon between them.
    SplitLevel,
}

/// A building's interior.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub scheme: Scheme,
    /// Footprint: width along the front, depth inwards.
    pub size: DVec2,
    pub spaces: Vec<Space>,
    pub links: Vec<Link>,
    pub stairs: Vec<Stair>,
    pub lifts: Vec<Lift>,
    pub columns: Vec<Column>,
}

impl Layout {
    /// The highest roof.
    pub fn height(&self) -> f64 {
        self.spaces.iter().map(Space::top).fold(0.0, f64::max)
    }

    /// The heights floors start at (not nested ones), lowest first.
    pub fn levels(&self) -> Vec<f64> {
        let mut ys: Vec<f64> = self
            .spaces
            .iter()
            .filter(|s| s.within.is_none())
            .map(|s| s.y)
            .collect();
        ys.sort_by(f64::total_cmp);
        ys.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
        ys
    }

    /// Everything wrong with the layout; empty when it is sound. The tests
    /// run this over many seeds and sizes, and the plan drawings print it.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.check_volume(&mut out);
        self.check_nested(&mut out);
        self.check_sizes(&mut out);
        self.check_links(&mut out);
        self.check_stairs(&mut out);
        self.check_lifts(&mut out);
        self.check_columns(&mut out);
        self.check_reach(&mut out);
        out
    }

    fn name(&self, i: usize) -> String {
        let s = &self.spaces[i];
        format!(
            "{:?}#{} ({:.1},{:.1})-({:.1},{:.1}) y {:.1}",
            s.kind, i, s.rect.min.x, s.rect.min.y, s.rect.max.x, s.rect.max.y, s.y
        )
    }

    /// Non-nested spaces fill the footprint: every plan column is covered
    /// from the ground up to its roof, with no gaps and no overlaps.
    fn check_volume(&self, out: &mut Vec<String>) {
        let top: Vec<&Space> = self.spaces.iter().filter(|s| s.within.is_none()).collect();
        let full = Rect::new(DVec2::ZERO, self.size);
        for s in &top {
            if !full.contains_rect(&s.rect, EPS) {
                out.push(format!("{:?} sticks out of the footprint", s.kind));
            }
        }
        let step = 0.5;
        let nx = (self.size.x / step).round() as i32;
        let nz = (self.size.y / step).round() as i32;
        for ix in 0..nx {
            for iz in 0..nz {
                let p = DVec2::new((ix as f64 + 0.5) * step, (iz as f64 + 0.5) * step);
                let mut spans: Vec<(f64, f64)> = top
                    .iter()
                    .filter(|s| inside(&s.rect, p))
                    .map(|s| (s.y, s.top()))
                    .collect();
                spans.sort_by(|a, b| a.0.total_cmp(&b.0));
                let mut y = 0.0;
                for (a, b) in &spans {
                    if (a - y).abs() > 1e-3 {
                        out.push(format!(
                            "at ({:.2},{:.2}) the floors go {:.2}..{:.2} after {:.2}",
                            p.x, p.y, a, b, y
                        ));
                        return;
                    }
                    y = *b;
                }
                if spans.is_empty() {
                    out.push(format!("nothing at ({:.2},{:.2})", p.x, p.y));
                    return;
                }
            }
        }
    }

    /// Nested spaces stand inside their host and not in each other.
    fn check_nested(&self, out: &mut Vec<String>) {
        for (i, s) in self.spaces.iter().enumerate() {
            let Some(h) = s.within else { continue };
            let host = &self.spaces[h];
            if host.within.is_some() {
                out.push(format!("{} is nested twice", self.name(i)));
            }
            if !host.rect.contains_rect(&s.rect, EPS)
                || s.y < host.y - EPS
                || s.top() > host.top() + EPS
            {
                out.push(format!("{} sticks out of {}", self.name(i), self.name(h)));
            }
            for (j, t) in self.spaces.iter().enumerate().skip(i + 1) {
                if t.within == Some(h)
                    && s.rect.overlap(&t.rect) > 1e-3
                    && s.y < t.top() - EPS
                    && t.y < s.top() - EPS
                {
                    out.push(format!("{} overlaps {}", self.name(i), self.name(j)));
                }
            }
        }
    }

    fn check_sizes(&self, out: &mut Vec<String>) {
        for (i, s) in self.spaces.iter().enumerate() {
            // A concourse runs inwards: only its width counts.
            let small = if s.kind == SpaceKind::Concourse {
                s.rect.size().x
            } else {
                s.rect.size().min_element()
            };
            let need = match s.kind {
                SpaceKind::Hall | SpaceKind::Lobby | SpaceKind::Atrium | SpaceKind::Boardroom => {
                    MIN_HALL
                }
                SpaceKind::Gallery
                | SpaceKind::Bridge
                | SpaceKind::Concourse
                | SpaceKind::Arcade
                | SpaceKind::Vestibule => MIN_WAY,
                _ => 0.0,
            };
            if small < need - EPS {
                out.push(format!("{} is only {:.1} m across", self.name(i), small));
            }
            if s.h < HEADROOM && s.kind != SpaceKind::Bridge && s.kind != SpaceKind::Landing {
                out.push(format!("{} is only {:.1} m high", self.name(i), s.h));
            }
        }
    }

    /// Links lie on a shared edge of their two spaces, at a height both can
    /// stand at (or see across, for rails and glass).
    fn check_links(&self, out: &mut Vec<String>) {
        let full = Rect::new(DVec2::ZERO, self.size);
        for l in &self.links {
            let (p, q) = l.ends();
            let a = &self.spaces[l.a];
            let on = |r: &Rect| on_edge(r, p, q, l.along_x);
            // A door from a nested space into its own host lies on the
            // nested space's wall, inside the host.
            let inner = |x: usize, y: usize| self.spaces[x].within == Some(y);
            if let Some(b) = l.b {
                if inner(l.a, b) || inner(b, l.a) {
                    let n = if inner(l.a, b) { l.a } else { b };
                    if !on(&self.spaces[n].rect) {
                        out.push(format!("door is not on the wall of {}", self.name(n)));
                    }
                    self.check_link_heights(l, out);
                    continue;
                }
            }
            if !on(&a.rect) {
                out.push(format!(
                    "{:?} link is not on the edge of {}",
                    l.kind,
                    self.name(l.a)
                ));
            }
            match l.b {
                Some(b) => {
                    if !on(&self.spaces[b].rect) {
                        out.push(format!(
                            "{:?} link is not on the edge of {}",
                            l.kind,
                            self.name(b)
                        ));
                    }
                }
                None => {
                    if !on(&full) {
                        out.push(format!("street {:?} is not on the outside wall", l.kind));
                    }
                }
            }
            self.check_link_heights(l, out);
        }
    }

    fn check_link_heights(&self, l: &Link, out: &mut Vec<String>) {
        for s in std::iter::once(l.a).chain(l.b) {
            let sp = &self.spaces[s];
            let ok = if l.kind.passable() {
                self.stands(s, l.y) && l.y + HEADROOM <= sp.top() + EPS
            } else {
                l.y >= sp.y - EPS && l.y < sp.top()
            };
            if !ok {
                out.push(format!(
                    "{:?} link at {:.2} m does not fit {}",
                    l.kind,
                    l.y,
                    self.name(s)
                ));
            }
        }
    }

    /// Can one stand in space `s` at height `y`? On its floor, on a nested
    /// space's floor, or on a stair landing or the top of a flight there.
    pub fn stands(&self, s: usize, y: f64) -> bool {
        let near = |v: f64| (v - y).abs() < 1e-3;
        let sp = &self.spaces[s];
        if near(sp.y)
            || self
                .lifts
                .iter()
                .any(|l| l.shaft == s && l.stops.iter().any(|v| near(*v)))
        {
            return true;
        }
        let host = sp.within.unwrap_or(s);
        self.spaces
            .iter()
            .any(|t| t.within == Some(host) && near(t.y))
            || self.stairs.iter().filter(|st| st.host == host).any(|st| {
                st.landings.iter().any(|l| near(l.y))
                    || st.flights.iter().any(|f| near(f.y0()) || near(f.y1()))
            })
    }

    /// Stairs stand inside their host, rise evenly, have headroom, start and
    /// end on something, and keep doorways clear.
    fn check_stairs(&self, out: &mut Vec<String>) {
        for (k, st) in self.stairs.iter().enumerate() {
            let host = &self.spaces[st.host];
            let name = format!("{:?} stair {} in {}", st.kind, k, self.name(st.host));
            let mut floors: Vec<f64> = st.landings.iter().map(|l| l.y).collect();
            floors.push(host.y);
            floors.extend(
                self.links
                    .iter()
                    .filter(|l| l.kind.passable() && (l.a == st.host || l.b == Some(st.host)))
                    .map(|l| l.y),
            );
            floors.extend(
                self.spaces
                    .iter()
                    .filter(|s| s.within == Some(st.host))
                    .map(|s| s.y),
            );
            // Doorways on the host's floor stay clear of the steps; a long
            // open edge needs one door's width clear somewhere.
            for l in &self.links {
                let mine = l.a == st.host || l.b == Some(st.host);
                if !mine || !l.kind.passable() {
                    continue;
                }
                let steps: Vec<&Flight> = st
                    .flights
                    .iter()
                    .filter(|f| !self.flight_meets(f, l) && (f.y1() - l.y).abs() > 1e-3)
                    .collect();
                let hit = |r: &Rect| steps.iter().any(|f| covers(f, r, l.y, l.y + HEADROOM));
                let apron = l.apron();
                let blocked = if l.width > 2.0 * DOUBLE {
                    let (a, b) = l.ends();
                    let dir = (b - a) / l.width;
                    let n = ((l.width - DOUBLE) / 0.5) as i32;
                    (0..=n).all(|k| {
                        let p = a + dir * (k as f64 * 0.5);
                        let q = p + dir * DOUBLE;
                        let side = dir.perp() * APRON;
                        let (u, v) = (p - side, q + side);
                        hit(&Rect::new(u.min(v), u.max(v)))
                    })
                } else {
                    hit(&apron)
                };
                if blocked {
                    out.push(format!(
                        "{name}: the stair blocks a {:?} at {:.1} m",
                        l.kind, l.y
                    ));
                }
            }
            for (i, f) in st.flights.iter().enumerate() {
                let fp = f.footprint();
                if !host.rect.contains_rect(&fp, 1e-3) {
                    out.push(format!("{name}: flight {i} leaves the hall"));
                }
                let rise = match *f {
                    Flight::Straight { rise, .. } | Flight::Arc { rise, .. } => rise,
                };
                if rise > RISE + EPS {
                    out.push(format!("{name}: flight {i} has {rise:.3} m risers"));
                }
                for y in [f.y0(), f.y1()] {
                    if !floors.iter().any(|v| (v - y).abs() < 1e-3) {
                        out.push(format!("{name}: flight {i} ends in the air at {y:.2}"));
                    }
                }
                if f.y1() + HEADROOM > host.top() + EPS {
                    out.push(format!("{name}: flight {i} has no headroom under the roof"));
                }
                // Headroom under every landing, bridge and flight above.
                for (j, g) in st.flights.iter().enumerate() {
                    if j != i {
                        self.flight_headroom(f, g, &name, out);
                    }
                }
                for l in &st.landings {
                    self.headroom(f, &l.rect, l.y, &name, out);
                }
                for s in self.spaces.iter().filter(|s| s.within == Some(st.host)) {
                    self.headroom(f, &s.rect, s.y, &name, out);
                }
            }
        }
    }

    /// Is `l` the doorway flight `f` starts from (its bottom step faces it)?
    fn flight_meets(&self, f: &Flight, l: &Link) -> bool {
        if (f.y0() - l.y).abs() > 1e-3 {
            return false;
        }
        match *f {
            Flight::Straight { from, .. } => l.apron().contains_point(DVec2::new(from.x, from.z)),
            Flight::Arc { .. } => false,
        }
    }

    /// Flight `g` passing over flight `f` leaves headroom.
    fn flight_headroom(&self, f: &Flight, g: &Flight, name: &str, out: &mut Vec<String>) {
        let (a, b) = (f.footprint(), g.footprint());
        let lo = a.min.max(b.min);
        let hi = a.max.min(b.max);
        if hi.x <= lo.x + 1e-3 || hi.y <= lo.y + 1e-3 {
            return;
        }
        let n = 12;
        for i in 0..n {
            for j in 0..n {
                let t = (DVec2::new(i as f64, j as f64) + 0.5) / n as f64;
                let p = lo + (hi - lo) * t;
                if let (Some(hf), Some(hg)) = (f.height_at(p), g.height_at(p)) {
                    if hg > hf + 1e-3 && hg < hf + HEADROOM {
                        out.push(format!(
                            "{name}: {:.1} m headroom under a flight at ({:.1},{:.1})",
                            hg - hf,
                            p.x,
                            p.y
                        ));
                        return;
                    }
                }
            }
        }
    }

    /// Something over flight `f` at height `y` (plan `r`) leaves headroom.
    fn headroom(&self, f: &Flight, r: &Rect, y: f64, name: &str, out: &mut Vec<String>) {
        let fp = f.footprint();
        if fp.overlap(r) < 1e-3 || y <= f.y0() + 1e-3 {
            return;
        }
        let lo = fp.min.max(r.min);
        let hi = fp.max.min(r.max);
        let n = 8;
        for i in 0..n {
            for j in 0..n {
                let t = (DVec2::new(i as f64, j as f64) + 0.5) / n as f64;
                let p = lo + (hi - lo) * t;
                if let Some(h) = f.height_at(p) {
                    if h < y - 1e-3 && h + HEADROOM > y + 1e-3 {
                        out.push(format!(
                            "{name}: {:.1} m headroom at ({:.1},{:.1})",
                            y - h,
                            p.x,
                            p.y
                        ));
                        return;
                    }
                }
            }
        }
    }

    fn check_lifts(&self, out: &mut Vec<String>) {
        for lift in &self.lifts {
            let s = &self.spaces[lift.shaft];
            if s.kind != SpaceKind::Shaft {
                out.push(format!("{:?} lift is not in a shaft", lift.kind));
            }
            for y in &lift.stops {
                let door = self.links.iter().any(|l| {
                    l.kind.passable()
                        && (l.y - y).abs() < 1e-3
                        && (l.a == lift.shaft || l.b == Some(lift.shaft))
                });
                if !door {
                    out.push(format!("{:?} lift has no door at {y:.2}", lift.kind));
                }
            }
        }
    }

    fn check_columns(&self, out: &mut Vec<String>) {
        for c in &self.columns {
            let p = c.at;
            for l in self.links.iter().filter(|l| l.kind.passable()) {
                if (l.y - c.y).abs() < 1e-3 && l.apron().inset(-c.r).contains_point(p) {
                    out.push(format!("a column at ({:.1},{:.1}) blocks a door", p.x, p.y));
                }
            }
            for st in &self.stairs {
                for f in &st.flights {
                    if f.footprint().inset(-c.r - 0.5).contains_point(p) {
                        out.push(format!("a column at ({:.1},{:.1}) is on a stair", p.x, p.y));
                    }
                }
            }
        }
    }

    /// Every space can be walked to from the street.
    fn check_reach(&self, out: &mut Vec<String>) {
        let reach = self.reachable();
        for (i, s) in self.spaces.iter().enumerate() {
            if s.kind == SpaceKind::Shaft {
                continue;
            }
            if !reach.iter().any(|(j, y)| *j == i && (y - s.y).abs() < 1e-3) {
                out.push(format!("{} cannot be reached", self.name(i)));
            }
        }
    }

    /// The (space, floor height) places one can walk to from the street.
    pub fn reachable(&self) -> Vec<(usize, f64)> {
        let key = |s: usize, y: f64| (s, (y * 1000.0).round() as i64);
        let mut seen: Vec<(usize, i64)> = Vec::new();
        let mut todo: Vec<(usize, f64)> = self
            .links
            .iter()
            .filter(|l| l.b.is_none() && l.kind.passable())
            .map(|l| (l.a, l.y))
            .collect();
        let mut out = Vec::new();
        while let Some((s, y)) = todo.pop() {
            if seen.contains(&key(s, y)) {
                continue;
            }
            seen.push(key(s, y));
            out.push((s, y));
            let near = |v: f64| (v - y).abs() < 1e-3;
            // Through links at this height.
            for l in self.links.iter().filter(|l| l.kind.passable() && near(l.y)) {
                if l.a == s {
                    if let Some(b) = l.b {
                        todo.push((b, y));
                    }
                } else if l.b == Some(s) {
                    todo.push((l.a, y));
                }
            }
            // Nested floors and their host at the same height.
            let sp = &self.spaces[s];
            if let Some(h) = sp.within {
                if sp.kind != SpaceKind::Shaft {
                    todo.push((h, y));
                }
            }
            for (j, t) in self.spaces.iter().enumerate() {
                if t.within == Some(s) && near(t.y) && t.kind != SpaceKind::Shaft {
                    todo.push((j, y));
                }
            }
            // Up and down the stairs in this space.
            for st in self.stairs.iter().filter(|st| st.host == s) {
                for f in &st.flights {
                    if near(f.y0()) {
                        todo.push((s, f.y1()));
                    }
                    if near(f.y1()) {
                        todo.push((s, f.y0()));
                    }
                }
            }
            // Lifts.
            for lift in self.lifts.iter().filter(|l| l.shaft == s) {
                if lift.stops.iter().any(|v| near(*v)) {
                    for v in &lift.stops {
                        todo.push((s, *v));
                    }
                }
            }
        }
        out
    }
}

/// Do the steps of `f` stand anywhere on `r`, between `above` and `below`?
fn covers(f: &Flight, r: &Rect, above: f64, below: f64) -> bool {
    let lo = f.footprint().min.max(r.min);
    let hi = f.footprint().max.min(r.max);
    if hi.x <= lo.x + 1e-3 || hi.y <= lo.y + 1e-3 {
        return false;
    }
    let n = 10;
    (0..n).any(|i| {
        (0..n).any(|j| {
            let t = (DVec2::new(i as f64, j as f64) + 0.5) / n as f64;
            f.height_at(lo + (hi - lo) * t)
                .is_some_and(|h| h > above - 1e-3 && h < below)
        })
    })
}

/// Half-open containment, so neighbouring rectangles never both count.
fn inside(r: &Rect, p: DVec2) -> bool {
    p.x >= r.min.x && p.x < r.max.x && p.y >= r.min.y && p.y < r.max.y
}

/// Does segment p-q lie on one edge of `r`?
fn on_edge(r: &Rect, p: DVec2, q: DVec2, along_x: bool) -> bool {
    let e = 1e-3;
    if along_x {
        let z = p.y;
        ((z - r.min.y).abs() < e || (z - r.max.y).abs() < e)
            && p.x.min(q.x) >= r.min.x - e
            && p.x.max(q.x) <= r.max.x + e
    } else {
        let x = p.x;
        ((x - r.min.x).abs() < e || (x - r.max.x).abs() < e)
            && p.y.min(q.y) >= r.min.y - e
            && p.y.max(q.y) <= r.max.y + e
    }
}

/// Collects a layout while a planner builds it.
#[derive(Default)]
pub(crate) struct Builder {
    pub spaces: Vec<Space>,
    pub links: Vec<Link>,
    pub stairs: Vec<Stair>,
    pub lifts: Vec<Lift>,
    pub columns: Vec<Column>,
}

impl Builder {
    pub fn space(&mut self, kind: SpaceKind, rect: Rect, y: f64, h: f64) -> usize {
        self.spaces.push(Space {
            kind,
            rect,
            y,
            h,
            within: None,
        });
        self.spaces.len() - 1
    }

    pub fn nested(&mut self, host: usize, kind: SpaceKind, rect: Rect, y: f64, h: f64) -> usize {
        self.spaces.push(Space {
            kind,
            rect,
            y,
            h,
            within: Some(host),
        });
        self.spaces.len() - 1
    }

    /// The edge `a` and `b` share: (along_x, wall coordinate, from, to).
    pub fn shared(&self, a: usize, b: usize) -> Option<(bool, f64, f64, f64)> {
        let (r, s) = (&self.spaces[a].rect, &self.spaces[b].rect);
        let e = 1e-3;
        for (ra, sb) in [(r.max.y, s.min.y), (r.min.y, s.max.y)] {
            if (ra - sb).abs() < e {
                let (lo, hi) = (r.min.x.max(s.min.x), r.max.x.min(s.max.x));
                if hi > lo + e {
                    return Some((true, ra, lo, hi));
                }
            }
        }
        for (ra, sb) in [(r.max.x, s.min.x), (r.min.x, s.max.x)] {
            if (ra - sb).abs() < e {
                let (lo, hi) = (r.min.y.max(s.min.y), r.max.y.min(s.max.y));
                if hi > lo + e {
                    return Some((false, ra, lo, hi));
                }
            }
        }
        None
    }

    /// A doorway of `width` centred on the shared edge (or at `centre` along
    /// it, clamped to fit). Panics if the spaces don't touch: a planner bug.
    pub fn door(&mut self, a: usize, b: usize, kind: LinkKind, y: f64, width: f64) {
        self.door_at(a, b, kind, y, width, None);
    }

    pub fn door_at(
        &mut self,
        a: usize,
        b: usize,
        kind: LinkKind,
        y: f64,
        width: f64,
        centre: Option<f64>,
    ) {
        let (along_x, w, lo, hi) = self.shared(a, b).unwrap_or_else(|| {
            panic!(
                "{:?} and {:?} do not touch",
                self.spaces[a].rect, self.spaces[b].rect
            )
        });
        let width = width.min(hi - lo);
        let place = |c: f64| {
            let c = c.clamp(lo + width * 0.5, hi - width * 0.5);
            let at = if along_x {
                DVec2::new(c, w)
            } else {
                DVec2::new(w, c)
            };
            Link {
                a,
                b: Some(b),
                kind,
                y,
                at,
                along_x,
                width,
            }
        };
        let link = match centre {
            Some(c) => place(c),
            // Without a set place: the middle, or the nearest spot to it
            // that no stair stands in front of.
            None => {
                let mid = (lo + hi) * 0.5;
                let free = |l: &Link| {
                    !self.stairs.iter().any(|st| {
                        (st.host == a || st.host == b)
                            && st
                                .flights
                                .iter()
                                .any(|f| covers(f, &l.apron(), y, y + HEADROOM))
                    })
                };
                (0..((hi - lo) as i32 + 1))
                    .flat_map(|k| [mid + k as f64 * 0.5, mid - k as f64 * 0.5])
                    .map(place)
                    .find(|l| free(l))
                    .unwrap_or_else(|| place(mid))
            }
        };
        self.links.push(link);
    }

    /// A lift door from a nested shaft into the hall round it, centred at
    /// `at` on the shaft's wall.
    pub fn inner_door(
        &mut self,
        shaft: usize,
        hall: usize,
        y: f64,
        at: DVec2,
        along_x: bool,
        width: f64,
    ) {
        self.links.push(Link {
            a: shaft,
            b: Some(hall),
            kind: LinkKind::Door,
            y,
            at,
            along_x,
            width,
        });
    }

    /// The whole shared edge, open, railed or glazed.
    pub fn edge(&mut self, a: usize, b: usize, kind: LinkKind, y: f64) {
        let (_, _, lo, hi) = self.shared(a, b).expect("spaces do not touch");
        self.door(a, b, kind, y, hi - lo);
    }

    /// The shared edge between `lo` and `hi` along it.
    pub fn edge_part(&mut self, a: usize, b: usize, kind: LinkKind, y: f64, lo: f64, hi: f64) {
        self.door_at(a, b, kind, y, hi - lo, Some((lo + hi) * 0.5));
    }

    /// A way in from the street on the front wall (z = 0).
    pub fn entrance(&mut self, a: usize, x: f64, width: f64) {
        self.links.push(Link {
            a,
            b: None,
            kind: LinkKind::Entrance,
            y: 0.0,
            at: DVec2::new(x, 0.0),
            along_x: true,
            width,
        });
    }

    /// Columns on a grid in a big room, kept off doorways and stairs.
    pub fn columns(&mut self, s: usize, spacing: f64) {
        let sp = self.spaces[s].clone();
        let size = sp.rect.size();
        if size.min_element() < 14.0 {
            return;
        }
        let nx = (size.x / spacing).round().max(1.0) as i32;
        let nz = (size.y / spacing).round().max(1.0) as i32;
        for i in 1..nx {
            for j in 1..nz {
                let at = sp.rect.min
                    + DVec2::new(size.x * i as f64 / nx as f64, size.y * j as f64 / nz as f64);
                let r = 0.45;
                let blocked = self.links.iter().any(|l| {
                    l.kind.passable()
                        && (l.y - sp.y).abs() < 1e-3
                        && l.apron().inset(-r - 1.0).contains_point(at)
                }) || self.stairs.iter().any(|st| {
                    st.flights
                        .iter()
                        .any(|f| f.footprint().inset(-r - 1.0).contains_point(at))
                        || st
                            .landings
                            .iter()
                            .any(|l| l.rect.inset(-r).contains_point(at))
                }) || self
                    .spaces
                    .iter()
                    .any(|t| t.within == Some(s) && t.rect.inset(-r - 0.5).contains_point(at));
                if !blocked {
                    self.columns.push(Column {
                        at,
                        r,
                        y: sp.y,
                        h: sp.h,
                    });
                }
            }
        }
    }

    pub fn finish(self, scheme: Scheme, size: DVec2) -> Layout {
        Layout {
            scheme,
            size,
            spaces: self.spaces,
            links: self.links,
            stairs: self.stairs,
            lifts: self.lifts,
            columns: self.columns,
        }
    }
}
