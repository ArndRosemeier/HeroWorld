//! Life along the Aether Tube: the walkway between stations is not one
//! long pipe. Every [`SLOT`] metres along a line there is room for a nook
//! beside it (or round it): a way up to the street, a row of mole folk's
//! homes, a market cave the tube runs through, a tube workers' shop, a
//! mushroom shrine or a burrow the mole folk dug through the tube's wall.
//! Stretches of a line come in sections with their own paint and lights
//! ([`Livery`]), and the mole folk's sections carry their glyphs and lamps.
//!
//! A slot belongs to the tile that files the stretch it falls on, and what
//! it holds is a function of the line, the slot and the town alone, so
//! tiles never read each other. Nooks keep away from stations, the lab and
//! other lines, so two tiles' nooks never meet.

use super::tube::{TubeNet, TUBE_HALF, TUBE_HEIGHT};
use super::{
    datum, room, run, DeepEntrance, DeepLamp, EntranceKind, Fixture, FixtureKind, Plan, TileKey,
};
use crate::urban::spiral::Spiral;
use crate::urban::underground::{Void, VoidKind, SHAFT_HALF};
use crate::urban::world::TownWorld;
use crate::water::segment_distance;
use glam::{DVec2, DVec3};
use hw_core::Seed;

/// Metres between nook slots along a line.
pub const SLOT: f64 = 75.0;
/// Every this many slots, a way up to the street (where no station is).
const EXIT_EVERY: usize = 3;
/// Length of a section with one look.
pub const SECTION: f64 = 260.0;
/// Keep this far from stations (their halls, lifts and gates)...
const STATION_CLEAR: f64 = 100.0;
/// ...and exits this far (a station is a way up already).
const EXIT_STATION_CLEAR: f64 = 150.0;
/// Keep this far from other lines (nooks reach about 25 m from their own).
const OTHER_LINE_CLEAR: f64 = 55.0;

/// What a nook is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NookKind {
    /// A passage to a lift up to the street.
    Exit,
    /// A row of mole folk's round-doored homes in a cave beside the tube.
    Homes,
    /// A market cave the tube runs straight through.
    Market,
    /// The tube workers' shop: benches, a pump, crates.
    Workshop,
    /// A mushroom garden round a carved mole idol.
    Shrine,
    /// A burrow the mole folk dug through the tube's wall to a den.
    Burrow,
}

impl NookKind {
    /// Do mole folk live here?
    pub fn mole(self) -> bool {
        matches!(
            self,
            NookKind::Homes | NookKind::Market | NookKind::Shrine | NookKind::Burrow
        )
    }
}

/// A nook as planned in its tile.
#[derive(Clone, Debug, PartialEq)]
pub struct Nook {
    pub kind: NookKind,
    /// Its line and slot.
    pub line: usize,
    pub slot: usize,
    /// Where it opens off the walkway (on the line), and the walkway floor.
    pub mouth: DVec2,
    pub floor: f64,
    /// The way from the line into it (for a market: the line's way).
    pub out: DVec2,
    /// Its main room's void (in the tile) and middle.
    pub room: usize,
    pub centre: DVec2,
}

/// Someone who lives or works along the tube: where they stand, and
/// where they walk to and back from (if they do).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Folk {
    pub at: DVec3,
    pub facing: DVec2,
    pub to: Option<DVec2>,
    /// A mole person (else a townsperson: a tube worker, a shopper).
    pub mole: bool,
}

/// How a section of tube looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Livery {
    /// Which paint scheme (see the client's tube drawing).
    pub style: u8,
    /// How much the mole folk have made it theirs (0..1): glyphs, their
    /// lamps, their nooks.
    pub mole: f64,
    /// Lamp colour (linear-ish) and every how many stretches a lamp hangs.
    pub lamp: [f32; 3],
    pub lamp_every: u32,
}

/// Paint schemes.
pub const STYLES: u8 = 6;

/// Where along a line its points are (m from the first).
pub fn arcs(pts: &[DVec2]) -> Vec<f64> {
    let mut out = Vec::with_capacity(pts.len());
    let mut s = 0.0;
    for (i, p) in pts.iter().enumerate() {
        if i > 0 {
            s += p.distance(pts[i - 1]);
        }
        out.push(s);
    }
    out
}

impl TubeNet {
    /// The look of line `line` round its point `i`.
    pub fn livery(&self, seed: Seed, line: usize, i: usize) -> Livery {
        let l = &self.lines[line];
        let s = l.arcs.get(i).copied().unwrap_or(0.0);
        let section = (s / SECTION).floor() as i64;
        let mut r = seed
            .child("tube-livery")
            .child(line as u64)
            .cell(section, 0)
            .rng();
        let style = r.range_int(0, STYLES as i64) as u8;
        // Mole folk favour some stretches: a third or so are theirs.
        let mole = (r.unit() * 1.6 - 0.45).clamp(0.0, 1.0);
        let lamp = if mole > 0.55 {
            [1.0, 0.68, 0.32]
        } else {
            [
                [0.45, 0.9, 1.0],
                [1.0, 0.86, 0.62],
                [0.62, 1.0, 0.78],
                [0.9, 0.78, 1.0],
            ][r.index(4)]
        };
        let lamp_every = if r.chance(0.25) { 3 } else { 2 };
        Livery {
            style,
            mole,
            lamp,
            lamp_every,
        }
    }

    /// The slots along line `l` on its stretch `i` (from point `i` to the
    /// next): slot numbers with their distance along the line.
    pub fn slots_on(&self, l: usize, i: usize) -> Vec<(usize, f64)> {
        let line = &self.lines[l];
        let (a, b) = (line.arcs[i], line.arcs[i + 1]);
        let first = ((a / SLOT) - 0.5).ceil().max(0.0) as usize;
        let mut out = Vec::new();
        let mut k = first;
        loop {
            let s = (k as f64 + 0.5) * SLOT;
            if s >= b {
                break;
            }
            if s >= a {
                out.push((k, s));
            }
            k += 1;
        }
        out
    }

    /// The point, floor and way of line `l` at `s` metres along it.
    pub fn along(&self, l: usize, s: f64) -> (DVec2, f64, DVec2) {
        let line = &self.lines[l];
        let n = line.pts.len();
        let i = line
            .arcs
            .partition_point(|x| *x <= s)
            .saturating_sub(1)
            .min(n - 2);
        let (a, b) = (line.pts[i], line.pts[i + 1]);
        let len = (line.arcs[i + 1] - line.arcs[i]).max(1e-9);
        let t = ((s - line.arcs[i]) / len).clamp(0.0, 1.0);
        let f = line.floors[i] + (line.floors[i + 1] - line.floors[i]) * t;
        (a.lerp(b, t), f, (b - a).normalize_or(DVec2::X))
    }
}

/// The nooks on the stretches filed in this tile.
pub(super) fn plan_tile(plan: &mut Plan) {
    let w = plan.w;
    let net = w.tube();
    let k = plan.key();
    let Some(list) = net.stretches_in(k) else {
        return;
    };
    for &(l, i) in list {
        let (l, i) = (l as usize, i as usize);
        for (slot, s) in net.slots_on(l, i) {
            nook(plan, net, l, slot, s);
        }
    }
}

/// Where slot `slot` of line `l` (at `s` m) is, if it may hold anything:
/// its point, walkway floor and the line's way there, and how far the
/// nearest station is.
fn slot_site(w: &TownWorld, net: &TubeNet, l: usize, s: f64) -> Option<(DVec2, f64, DVec2, f64)> {
    let line = &net.lines[l];
    let total = *line.arcs.last().expect("a line has points");
    if s < 30.0 || total - s < 30.0 {
        return None;
    }
    let (p, floor, dir) = net.along(l, s);
    let near_station = net
        .stations
        .iter()
        .map(|st| st.pos.distance(p).min(st.hall_centre().distance(p)))
        .fold(f64::MAX, f64::min);
    if near_station < STATION_CLEAR {
        return None;
    }
    // Well clear of other lines (junctions and crossings) and the lab.
    let crowded = net
        .stretch_voids_near(p, OTHER_LINE_CLEAR)
        .iter()
        .any(|(ol, oi, _)| {
            *ol != l && {
                let o = &net.lines[*ol];
                segment_distance(p, o.pts[*oi], o.pts[*oi + 1]) < OTHER_LINE_CLEAR
            }
        });
    if crowded
        || w.lair_site()
            .is_some_and(|x| x.centre.distance(p) < super::lair::LAIR_REACH + 40.0)
    {
        return None;
    }
    // Well above the buried city's rooms.
    if floor < datum(w, p) - super::RUIN_DEPTH + 16.0 {
        return None;
    }
    Some((p, floor, dir, near_station))
}

/// Where the ways up planned with tile `k` come out on the street: what
/// [`plan_tile`] finds for exits, without the rest of the tile.
pub(super) fn exits_of_tile(w: &TownWorld, k: TileKey) -> Vec<DVec2> {
    let net = w.tube();
    let Some(list) = net.stretches_in(k) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for &(l, i) in list {
        let (l, i) = (l as usize, i as usize);
        for (slot, s) in net.slots_on(l, i) {
            if slot_site(w, net, l, s).is_none() {
                continue;
            }
            if let Some(site) = wanted_exit(w, net, l, slot) {
                out.push(site.c);
            }
        }
    }
    out
}

/// What, if anything, slot `slot` of line `l` (at `s` m) holds.
fn nook(plan: &mut Plan, net: &TubeNet, l: usize, slot: usize, s: f64) {
    let w = plan.w;
    let line = &net.lines[l];
    let Some((p, floor, dir, _)) = slot_site(w, net, l, s) else {
        return;
    };
    if let Some(site) = wanted_exit(w, net, l, slot) {
        place_exit(plan, site);
        return;
    }
    let seed = w.seed.child("tube-nook").child(l as u64).child(slot as u64);
    let mut r = seed.rng();
    let side = if r.chance(0.5) { 1.0 } else { -1.0 };
    let i = line
        .arcs
        .partition_point(|x| *x <= s)
        .saturating_sub(1)
        .min(line.pts.len() - 2);
    let liv = net.livery(w.seed, l, i);
    let m = liv.mole;
    // Level enough for a cave the walkway crosses?
    let (_, fa, _) = net.along(l, s - 14.0);
    let (_, fb, _) = net.along(l, s + 14.0);
    let level = (fa - floor).abs() < 0.5 && (fb - floor).abs() < 0.5;
    let weights = [
        0.32 * m + 0.04,                           // Homes
        if level { 0.16 * m + 0.02 } else { 0.0 }, // Market
        0.22 * (1.0 - m) + 0.07,                   // Workshop
        0.1 * m + 0.03,                            // Shrine
        0.14 * m + 0.02,                           // Burrow
        0.16 * (1.0 - m) + 0.05,                   // nothing: plain tube
    ];
    let kind = match r.weighted(&weights) {
        0 => NookKind::Homes,
        1 => NookKind::Market,
        2 => NookKind::Workshop,
        3 => NookKind::Shrine,
        4 => NookKind::Burrow,
        _ => return,
    };
    let ctx = Ctx {
        l,
        slot,
        p,
        floor,
        dir,
        out: dir.perp() * side,
        seed,
    };
    // A nook that does not fit tries the other side, then gives up.
    if !try_nook(plan, net, &ctx, kind) {
        let flipped = Ctx {
            out: -ctx.out,
            ..ctx
        };
        try_nook(plan, net, &flipped, kind);
    }
}

#[derive(Clone, Copy)]
struct Ctx {
    l: usize,
    slot: usize,
    p: DVec2,
    floor: f64,
    dir: DVec2,
    out: DVec2,
    seed: Seed,
}

impl Ctx {
    /// A point `x` along the line and `y` out from it.
    fn at(&self, x: f64, y: f64) -> DVec2 {
        self.p + self.dir * x + self.out * y
    }
    fn v3(&self, x: f64, y: f64) -> DVec3 {
        let q = self.at(x, y);
        DVec3::new(q.x, self.floor, q.y)
    }
}

/// Do these voids fit: under rock, clear of the shallow underground and of
/// the other stretches of the tube (bar the line's own, round the slot)?
fn fits(w: &TownWorld, net: &TubeNet, ctx: &Ctx, voids: &[Void]) -> bool {
    let others = net.stretch_voids_near(ctx.p, 60.0);
    let line = &net.lines[ctx.l];
    let under = w.core_underground();
    let shallow_clear = |v: &Void| {
        let (lo, hi) = super::bounds(v);
        let c = (lo + hi) * 0.5;
        under
            .near(c, (hi - lo).length() * 0.5 + 1.5)
            .into_iter()
            .all(|i| !super::overlaps(v, &under.voids[i], 1.5))
    };
    voids.iter().all(|v| {
        (v.open || super::roofed(w, v, 3.0))
            && shallow_clear(v)
            && others.iter().all(|(ol, oi, o)| {
                // The line's own tube right here is where the nook opens.
                let own = *ol == ctx.l && {
                    let m = (line.pts[*oi] + line.pts[*oi + 1]) * 0.5;
                    let d = segment_distance(ctx.p, line.pts[*oi], line.pts[*oi + 1]);
                    d < 1.0 || m.distance(ctx.p) < 32.0
                };
                own || !super::overlaps(v, o, 1.0)
            })
    })
}

fn push_fixture(plan: &mut Plan, kind: FixtureKind, at: DVec3, dir: DVec2, size: DVec3, look: u32) {
    plan.t.fixtures.push(Fixture {
        kind,
        at,
        dir: dir.normalize_or(DVec2::X),
        size,
        look,
    });
}

fn lamp(plan: &mut Plan, at: DVec3, color: [f32; 3], strength: f32, range: f32) {
    plan.t.lamps.push(DeepLamp {
        pos: at,
        color,
        strength,
        range,
    });
}

/// Warm mole lantern light.
const AMBER: [f32; 3] = [1.0, 0.66, 0.3];

fn try_nook(plan: &mut Plan, net: &TubeNet, ctx: &Ctx, kind: NookKind) -> bool {
    let c = ctx;
    let f = c.floor;
    let mut r = c.seed.child("fit").rng();
    let (voids, centre): (Vec<Void>, DVec2) = match kind {
        NookKind::Homes => {
            let hx = r.range(8.0, 10.5);
            let mouth_len = 3.0;
            let centre = c.at(0.0, TUBE_HALF + mouth_len + 3.6);
            let mut v = vec![room(
                VoidKind::Grotto,
                centre,
                c.dir,
                DVec2::new(hx, 3.6),
                f,
                3.8,
            )];
            v.extend(run(
                VoidKind::Gallery,
                c.p,
                c.at(0.0, TUBE_HALF + mouth_len + 0.5),
                1.5,
                f,
                f,
                3.0,
            ));
            (v, centre)
        }
        NookKind::Market => {
            let centre = c.p;
            (
                vec![room(
                    VoidKind::Grotto,
                    centre,
                    c.dir,
                    DVec2::new(14.0, 7.5),
                    f,
                    5.6,
                )],
                centre,
            )
        }
        NookKind::Workshop => {
            let centre = c.at(0.0, TUBE_HALF + 2.5 + 4.0);
            let mut v = vec![room(
                VoidKind::Service,
                centre,
                c.dir,
                DVec2::new(6.5, 4.0),
                f,
                3.6,
            )];
            v.extend(run(
                VoidKind::Passage,
                c.p,
                c.at(0.0, TUBE_HALF + 2.5 + 0.5),
                1.3,
                f,
                f,
                2.9,
            ));
            (v, centre)
        }
        NookKind::Shrine => {
            let centre = c.at(0.0, TUBE_HALF + 3.0 + 5.5);
            let mut v = vec![room(
                VoidKind::Grotto,
                centre,
                c.dir,
                DVec2::new(5.5, 5.5),
                f,
                4.8,
            )];
            v.extend(run(
                VoidKind::Gallery,
                c.p,
                c.at(0.0, TUBE_HALF + 3.5),
                1.4,
                f,
                f,
                3.0,
            ));
            (v, centre)
        }
        NookKind::Burrow => {
            // Out through the broken wall, a bend, and a den.
            let bend = r.range(-7.0, 7.0);
            let a = c.at(0.0, TUBE_HALF + 6.0);
            let b = c.at(bend, TUBE_HALF + 12.0);
            let centre = c.at(bend, TUBE_HALF + 12.0 + 4.0);
            let mut v = Vec::new();
            v.extend(run(VoidKind::Gallery, c.p, a, 1.4, f, f, 3.0));
            v.extend(run(VoidKind::Gallery, a, b, 1.4, f, f, 3.0));
            v.push(room(
                VoidKind::Grotto,
                centre,
                c.dir,
                DVec2::new(5.0, 4.0),
                f,
                3.6,
            ));
            (v, centre)
        }
        NookKind::Exit => unreachable!("exits are planned by `exit`"),
    };
    if !fits(plan.w, net, c, &voids) {
        return false;
    }
    let room_i = plan.t.voids.len()
        + voids
            .iter()
            .position(|v| !matches!(v.kind, VoidKind::Gallery | VoidKind::Passage))
            .unwrap_or(0);
    plan.t.voids.extend(voids);
    plan.t.nooks.push(Nook {
        kind,
        line: c.l,
        slot: c.slot,
        mouth: c.p,
        floor: f,
        out: if kind == NookKind::Market {
            c.dir
        } else {
            c.out
        },
        room: room_i,
        centre,
    });
    match kind {
        NookKind::Homes => homes(plan, c, &mut r),
        NookKind::Market => market(plan, c, &mut r),
        NookKind::Workshop => workshop(plan, c, &mut r),
        NookKind::Shrine => shrine(plan, c, centre, &mut r),
        NookKind::Burrow => burrow(plan, c, centre, &mut r),
        NookKind::Exit => {}
    }
    true
}

/// Round doors in the back wall, lanterns, benches and barrels, washing on
/// a line, glowing mushrooms; neighbours at their doors.
fn homes(plan: &mut Plan, c: &Ctx, r: &mut hw_core::seed::Rng) {
    let f = c.floor;
    let hx = plan.t.voids[plan.t.nooks.last().expect("this nook").room]
        .max
        .x;
    let back = TUBE_HALF + 3.0 + 7.2; // out from the line: the back wall
    let doors = if hx > 9.5 { 3 } else { 2 };
    let mut fronts = Vec::new();
    for d in 0..doors {
        let x = -hx + 2.0 * hx * (d as f64 + 0.5) / doors as f64 + r.range(-0.8, 0.8);
        let at = c.at(x, back - 0.05);
        push_fixture(
            plan,
            FixtureKind::RoundDoor,
            DVec3::new(at.x, f, at.y),
            -c.out,
            DVec3::new(0.95, 2.1, 0.0),
            r.range_int(0, 1 << 16) as u32,
        );
        lamp(
            plan,
            c.v3(x + 1.3, back - 0.6) + DVec3::Y * 2.3,
            AMBER,
            3.5,
            7.0,
        );
        fronts.push(c.at(x, back - 2.0));
        // Something by each door.
        let side = if r.chance(0.5) { 1.0 } else { -1.0 };
        let q = c.at(x + side * 1.8, back - 0.75);
        match r.index(3) {
            0 => push_fixture(
                plan,
                FixtureKind::Barrel,
                DVec3::new(q.x, f, q.y),
                c.dir,
                DVec3::new(0.38, 0.95, 0.0),
                r.range_int(0, 1 << 16) as u32,
            ),
            1 => push_fixture(
                plan,
                FixtureKind::Crate,
                DVec3::new(q.x, f, q.y),
                c.dir,
                DVec3::new(0.4, 0.7, 0.4),
                r.range_int(0, 1 << 16) as u32,
            ),
            _ => push_fixture(
                plan,
                FixtureKind::MushroomPot,
                DVec3::new(q.x, f, q.y),
                c.dir,
                DVec3::new(0.35, 0.6, 0.0),
                r.range_int(0, 1 << 16) as u32,
            ),
        }
    }
    // A lantern post either side of the mouth, a bench, washing overhead.
    for sx in [-3.2, 3.2] {
        let q = c.at(sx, TUBE_HALF + 3.4);
        push_fixture(
            plan,
            FixtureKind::Lantern,
            DVec3::new(q.x, f, q.y),
            c.out,
            DVec3::new(0.12, 2.4, 0.0),
            0,
        );
        lamp(plan, DVec3::new(q.x, f + 2.3, q.y), AMBER, 3.0, 7.0);
    }
    let q = c.at(-hx + 1.4, TUBE_HALF + 5.4);
    push_fixture(
        plan,
        FixtureKind::Bench,
        DVec3::new(q.x, f, q.y),
        c.dir,
        DVec3::new(1.0, 0.9, 0.3),
        1,
    );
    let (a, b) = (c.at(-hx + 1.0, back - 2.8), c.at(hx - 1.0, back - 2.4));
    push_fixture(
        plan,
        FixtureKind::Washing,
        DVec3::new((a.x + b.x) * 0.5, f + 2.55, (a.y + b.y) * 0.5),
        (b - a).normalize_or(c.dir),
        DVec3::new(a.distance(b) * 0.5, 0.0, 0.0),
        r.range_int(0, 1 << 16) as u32,
    );
    let q = c.at(hx - 1.2, TUBE_HALF + 4.2);
    push_fixture(
        plan,
        FixtureKind::Rug,
        DVec3::new(q.x, f + 0.01, q.y),
        c.dir,
        DVec3::new(1.0, 0.0, 0.7),
        r.range_int(0, 1 << 16) as u32,
    );
    // Neighbours: one at a door, two chatting, one off along the tube and back.
    let face = |from: DVec2, to: DVec2| (to - from).normalize_or(c.dir);
    if let Some(&d) = fronts.first() {
        plan.t.folk.push(Folk {
            at: DVec3::new(d.x, f, d.y),
            facing: -c.out,
            to: None,
            mole: true,
        });
    }
    let (m1, m2) = (c.at(1.0, TUBE_HALF + 4.6), c.at(1.8, TUBE_HALF + 5.4));
    plan.t.folk.push(Folk {
        at: DVec3::new(m1.x, f, m1.y),
        facing: face(m1, m2),
        to: None,
        mole: true,
    });
    plan.t.folk.push(Folk {
        at: DVec3::new(m2.x, f, m2.y),
        facing: face(m2, m1),
        to: None,
        mole: true,
    });
    if let Some(&d) = fronts.last() {
        // From home out to the walkway and back.
        plan.t.folk.push(Folk {
            at: DVec3::new(d.x, f, d.y),
            facing: -c.out,
            to: Some(c.at(0.0, TUBE_HALF + 1.6)),
            mole: true,
        });
    }
}

/// Striped stalls either side of the walkway, lanterns, keepers and
/// shoppers; the tube's shell opens out into the cave.
fn market(plan: &mut Plan, c: &Ctx, r: &mut hw_core::seed::Rng) {
    let f = c.floor;
    for s in [-1.0f64, 1.0] {
        for x in [-9.0, 0.0, 9.0] {
            if r.chance(0.2) {
                continue;
            }
            let x = x + r.range(-1.0, 1.0);
            let y = s * 5.2;
            let at = c.at(x, y);
            // From the stall towards the walkway.
            let facing = (c.at(x, 0.0) - at).normalize_or(c.out);
            push_fixture(
                plan,
                FixtureKind::Stall,
                DVec3::new(at.x, f, at.y),
                facing,
                DVec3::new(1.5, 2.6, 0.75),
                r.range_int(0, 1 << 16) as u32,
            );
            let keeper = at - facing * 1.25;
            let shopper = at + facing * 1.6 + facing.perp() * r.range(-0.6, 0.6);
            plan.t.folk.push(Folk {
                at: DVec3::new(keeper.x, f, keeper.y),
                facing,
                to: None,
                mole: true,
            });
            if r.chance(0.7) {
                plan.t.folk.push(Folk {
                    at: DVec3::new(shopper.x, f, shopper.y),
                    facing: -facing,
                    to: None,
                    mole: r.chance(0.7),
                });
            }
        }
        // Lantern posts between the stalls.
        for x in [-4.5, 4.5] {
            let at = c.at(x, s * 5.6);
            push_fixture(
                plan,
                FixtureKind::Lantern,
                DVec3::new(at.x, f, at.y),
                c.dir,
                DVec3::new(0.12, 2.6, 0.0),
                1,
            );
        }
        // Barrels and crates at the ends.
        for x in [-12.4, 12.4] {
            let at = c.at(x, s * 5.6);
            push_fixture(
                plan,
                FixtureKind::Barrel,
                DVec3::new(at.x, f, at.y),
                c.dir,
                DVec3::new(0.38, 0.95, 0.0),
                r.range_int(0, 1 << 16) as u32,
            );
            let at = c.at(x + 0.9_f64.copysign(-x), s * 6.4);
            push_fixture(
                plan,
                FixtureKind::Crate,
                DVec3::new(at.x, f, at.y),
                c.dir,
                DVec3::new(0.42, 0.75, 0.42),
                r.range_int(0, 1 << 16) as u32,
            );
        }
    }
    for x in [-9.0, 0.0, 9.0] {
        lamp(plan, c.v3(x, 0.0) + DVec3::Y * 4.4, AMBER, 6.0, 13.0);
    }
    // Bunting of lamps across the cave.
    push_fixture(
        plan,
        FixtureKind::LampString,
        c.v3(0.0, 0.0) + DVec3::Y * 4.6,
        c.dir,
        DVec3::new(13.0, 0.0, 6.5),
        r.range_int(0, 1 << 16) as u32,
    );
    // Shoppers walking through along the walkway's sides.
    for s in [-1.0, 1.0] {
        let a = c.at(-12.0, s * 2.6);
        let b = c.at(12.0, s * 2.6);
        plan.t.folk.push(Folk {
            at: DVec3::new(a.x, f, a.y),
            facing: c.dir,
            to: Some(b),
            mole: r.chance(0.6),
        });
    }
}

/// The tube workers' shop: a pump with gauges, a bench, pipes, crates and
/// a cart; a lamp over the bench.
fn workshop(plan: &mut Plan, c: &Ctx, r: &mut hw_core::seed::Rng) {
    let f = c.floor;
    let back = TUBE_HALF + 2.5 + 8.0;
    let bench = c.at(-2.0, back - 0.75);
    push_fixture(
        plan,
        FixtureKind::Workbench,
        DVec3::new(bench.x, f, bench.y),
        -c.out,
        DVec3::new(1.4, 0.95, 0.45),
        r.range_int(0, 1 << 16) as u32,
    );
    let pump = c.at(4.0, back - 1.4);
    push_fixture(
        plan,
        FixtureKind::Pump,
        DVec3::new(pump.x, f, pump.y),
        -c.out,
        DVec3::new(0.7, 2.2, 0.0),
        r.range_int(0, 1 << 16) as u32,
    );
    let cart = c.at(-5.0, TUBE_HALF + 4.2);
    push_fixture(
        plan,
        FixtureKind::Cart,
        DVec3::new(cart.x, f, cart.y),
        c.out,
        DVec3::new(0.55, 1.0, 0.8),
        r.range_int(0, 1 << 16) as u32,
    );
    for (x, y) in [(5.0, TUBE_HALF + 3.4), (5.0, TUBE_HALF + 4.3)] {
        let q = c.at(x, y);
        push_fixture(
            plan,
            FixtureKind::Crate,
            DVec3::new(q.x, f, q.y),
            c.dir,
            DVec3::new(0.4, 0.75, 0.4),
            r.range_int(0, 1 << 16) as u32,
        );
    }
    let q = c.at(-5.6, back - 0.7);
    push_fixture(
        plan,
        FixtureKind::Barrel,
        DVec3::new(q.x, f, q.y),
        c.dir,
        DVec3::new(0.38, 0.95, 0.0),
        r.range_int(0, 1 << 16) as u32,
    );
    lamp(
        plan,
        c.v3(-2.0, back - 1.6) + DVec3::Y * 3.1,
        [1.0, 0.92, 0.75],
        5.0,
        10.0,
    );
    lamp(
        plan,
        c.v3(3.0, TUBE_HALF + 4.0) + DVec3::Y * 3.1,
        [0.75, 0.95, 1.0],
        4.0,
        9.0,
    );
    // A worker at the bench, one at the pump.
    let w1 = c.at(-2.0, back - 1.8);
    plan.t.folk.push(Folk {
        at: DVec3::new(w1.x, f, w1.y),
        facing: c.out,
        to: None,
        mole: false,
    });
    let w2 = c.at(4.0, back - 3.0);
    plan.t.folk.push(Folk {
        at: DVec3::new(w2.x, f, w2.y),
        facing: c.out,
        to: Some(c.at(0.0, TUBE_HALF + 2.2)),
        mole: r.chance(0.3),
    });
}

/// A carved mole idol among glowing mushrooms, by a glow pool; candles.
fn shrine(plan: &mut Plan, c: &Ctx, centre: DVec2, r: &mut hw_core::seed::Rng) {
    let f = c.floor;
    let idol = c.at(0.0, TUBE_HALF + 3.0 + 9.2);
    push_fixture(
        plan,
        FixtureKind::Idol,
        DVec3::new(idol.x, f, idol.y),
        -c.out,
        DVec3::new(0.9, 2.6, 0.0),
        r.range_int(0, 1 << 16) as u32,
    );
    lamp(
        plan,
        DVec3::new(idol.x, f + 1.2, idol.y) - DVec3::new(c.out.x, 0.0, c.out.y) * 1.2,
        [1.0, 0.6, 0.25],
        4.0,
        8.0,
    );
    // Giant mushrooms in the back corners, small ones along the sides.
    let o = TUBE_HALF + 3.0;
    for (k, (x, y)) in [
        (-3.9, o + 9.0),
        (3.9, o + 9.0),
        (-4.3, o + 5.2),
        (4.3, o + 5.6),
        (-2.2, o + 9.9),
    ]
    .into_iter()
    .enumerate()
    {
        let q = c.at(x, y);
        let (kind, size) = if k < 2 {
            (FixtureKind::GiantMushroom, DVec3::new(1.1, 2.4, 0.0))
        } else {
            (FixtureKind::MushroomPot, DVec3::new(0.4, 0.7, 0.0))
        };
        push_fixture(
            plan,
            kind,
            DVec3::new(q.x, f, q.y),
            c.dir,
            size,
            r.range_int(0, 1 << 16) as u32,
        );
    }
    let pool = c.at(-2.6, TUBE_HALF + 3.0 + 4.0);
    push_fixture(
        plan,
        FixtureKind::GlowPool,
        DVec3::new(pool.x, f, pool.y),
        c.dir,
        DVec3::new(1.4, 0.0, 1.0),
        0,
    );
    lamp(
        plan,
        DVec3::new(pool.x, f + 0.6, pool.y),
        [0.4, 1.0, 0.8],
        3.0,
        7.0,
    );
    let q = c.at(1.5, TUBE_HALF + 3.0 + 7.6);
    push_fixture(
        plan,
        FixtureKind::Candles,
        DVec3::new(q.x, f, q.y),
        -c.out,
        DVec3::new(0.5, 0.3, 0.3),
        0,
    );
    // A mole at prayer, one tending the mushrooms.
    let a = c.at(0.3, TUBE_HALF + 3.0 + 6.4);
    plan.t.folk.push(Folk {
        at: DVec3::new(a.x, f, a.y),
        facing: c.out,
        to: None,
        mole: true,
    });
    if r.chance(0.6) {
        let b = c.at(2.6, TUBE_HALF + 3.0 + 3.0);
        plan.t.folk.push(Folk {
            at: DVec3::new(b.x, f, b.y),
            facing: (centre - b).normalize_or(c.out),
            to: None,
            mole: true,
        });
    }
}

/// The broken tube wall, rubble, picks and crates down the burrow; a den
/// with tents round a fire.
fn burrow(plan: &mut Plan, c: &Ctx, centre: DVec2, r: &mut hw_core::seed::Rng) {
    let f = c.floor;
    for (x, y) in [(-1.4, TUBE_HALF + 2.2), (1.3, TUBE_HALF + 3.4)] {
        let q = c.at(x, y);
        push_fixture(
            plan,
            FixtureKind::Rubble,
            DVec3::new(q.x, f, q.y),
            c.dir,
            DVec3::new(0.6, 0.45, 0.5),
            r.range_int(0, 1 << 16) as u32,
        );
    }
    // Glyphs either side of the breach, on the tube.
    for sx in [-2.6, 2.6] {
        let q = c.at(sx, TUBE_HALF + 0.25);
        push_fixture(
            plan,
            FixtureKind::Glyphs,
            DVec3::new(q.x, f + 1.0, q.y),
            -c.out,
            DVec3::new(0.9, 0.8, 0.0),
            r.range_int(0, 1 << 16) as u32,
        );
    }
    let fire = centre;
    push_fixture(
        plan,
        FixtureKind::Campfire,
        DVec3::new(fire.x, f, fire.y),
        c.dir,
        DVec3::new(0.5, 0.4, 0.5),
        0,
    );
    lamp(
        plan,
        DVec3::new(fire.x, f + 0.8, fire.y),
        [1.0, 0.55, 0.2],
        5.0,
        9.0,
    );
    let tent = centre + c.out * 2.4 - c.dir * 2.2;
    push_fixture(
        plan,
        FixtureKind::Tent,
        DVec3::new(tent.x, f, tent.y),
        -c.out,
        DVec3::new(1.1, 1.6, 1.0),
        r.range_int(0, 1 << 16) as u32,
    );
    let crates = centre + c.out * 2.6 + c.dir * 2.8;
    push_fixture(
        plan,
        FixtureKind::Crate,
        DVec3::new(crates.x, f, crates.y),
        c.dir,
        DVec3::new(0.4, 0.7, 0.4),
        r.range_int(0, 1 << 16) as u32,
    );
    let cart = centre - c.out * 2.4 + c.dir * 2.6;
    push_fixture(
        plan,
        FixtureKind::Cart,
        DVec3::new(cart.x, f, cart.y),
        c.dir,
        DVec3::new(0.55, 1.0, 0.8),
        r.range_int(0, 1 << 16) as u32,
    );
    // Diggers round the fire.
    for k in 0..3 {
        let a = std::f64::consts::TAU * (k as f64 / 3.0 + 0.1);
        let q = fire + (c.dir * a.cos() + c.out * a.sin()) * 1.6;
        plan.t.folk.push(Folk {
            at: DVec3::new(q.x, f, q.y),
            facing: (fire - q).normalize_or(c.dir),
            to: None,
            mole: true,
        });
    }
}

/// A way up as planned: the passage from the walkway and the stair shaft.
struct ExitSite {
    ctx: Ctx,
    c: DVec2,
    passage: Void,
    shaft: Void,
    spiral: Spiral,
}

/// Does slot `slot` of line `l` get a way up? Every [`EXIT_EVERY`]th slot
/// tries for one (away from stations); where it finds no room on the
/// street, the next slots try in turn. A function of the line, the slot and
/// the town alone, so any tile can tell.
fn wanted_exit(w: &TownWorld, net: &TubeNet, l: usize, slot: usize) -> Option<ExitSite> {
    let offset = (slot + EXIT_EVERY - 1) % EXIT_EVERY;
    for before in slot.saturating_sub(offset)..slot {
        if exit_site(w, net, l, before).is_some() {
            return None;
        }
    }
    exit_site(w, net, l, slot)
}

/// Where a way up off slot `slot` of line `l` would go, if it fits.
fn exit_site(w: &TownWorld, net: &TubeNet, l: usize, slot: usize) -> Option<ExitSite> {
    let s = (slot as f64 + 0.5) * SLOT;
    let (p, floor, dir, near_station) = slot_site(w, net, l, s)?;
    if near_station < EXIT_STATION_CLEAR {
        return None;
    }
    let line = &net.lines[l];
    let seed = w.seed.child("tube-exit").child(l as u64).child(slot as u64);
    let side = if seed.rng().chance(0.5) { 1.0 } else { -1.0 };
    // Nearest first: out of the kerb, then along the road, both sides.
    let mut tries: Vec<(f64, f64, f64)> = Vec::new();
    for sd in [side, -side] {
        for x in [
            0.0, 6.0, -6.0, 12.0, -12.0, 18.0, -18.0, 24.0, -24.0, 30.0, -30.0,
        ] {
            for e in [0.0, 3.0, 6.0, 9.0, 12.0] {
                tries.push((sd, x, e));
            }
        }
    }
    tries.sort_by(|a, b| {
        let k = |t: &(f64, f64, f64)| t.1.abs() + t.2 * 2.0 + if t.0 == side { 0.0 } else { 0.5 };
        k(a).total_cmp(&k(b))
    });
    let ctx_of = |sd: f64| Ctx {
        l,
        slot,
        p,
        floor,
        dir,
        out: dir.perp() * sd,
        seed,
    };
    let spots: Vec<(DVec2, f64)> = tries
        .iter()
        .map(|&(sd, x, e)| {
            (
                ctx_of(sd).at(x, line.verge + SHAFT_HALF + 1.6 + e),
                SHAFT_HALF + 1.2,
            )
        })
        .collect();
    let reach = DVec2::splat(line.verge + 30.0 + 12.0);
    let free = super::super::underground::spots_free(w, p - reach, p + reach, &spots);
    for (k, &(sd, _, _)) in tries.iter().enumerate() {
        if !free[k] {
            continue;
        }
        let ctx = ctx_of(sd);
        let c = spots[k].0;
        let Some(passage) = run(VoidKind::Passage, p, c, 1.5, floor, floor, 3.2) else {
            continue;
        };
        // The door faces the road, away from the walkway.
        let Some(spiral) = Spiral::fit(c, -ctx.out, w.ground(c), floor, Some(&passage)) else {
            continue;
        };
        let shaft = spiral.shaft(None);
        if !fits(w, net, &ctx, std::slice::from_ref(&passage))
            || !fits(w, net, &ctx, std::slice::from_ref(&shaft))
        {
            continue;
        }
        return Some(ExitSite {
            ctx,
            c,
            passage,
            shaft,
            spiral,
        });
    }
    None
}

/// A way up: a short passage off the walkway to a stair up to a little
/// green pavilion on the pavement, its sign glowing in the tube.
fn place_exit(plan: &mut Plan, site: ExitSite) {
    let w = plan.w;
    let ExitSite {
        ctx,
        c,
        passage,
        shaft,
        spiral,
    } = site;
    let (mouth, floor, dir, out) = (ctx.p, ctx.floor, ctx.dir, ctx.out);
    let room = plan.t.voids.len();
    plan.t.voids.push(shaft);
    plan.t.voids.push(passage);
    plan.t.entrances.push(DeepEntrance {
        kind: EntranceKind::Exit,
        name: "Tube exit".into(),
        pos: c,
        door: -out,
        top: w.ground(c),
        bottom: floor,
        spiral,
    });
    plan.t.nooks.push(Nook {
        kind: NookKind::Exit,
        line: ctx.l,
        slot: ctx.slot,
        mouth,
        floor,
        out: (c - mouth).normalize_or(out),
        room,
        centre: c,
    });
    // The sign over the mouth, read from both ways along the tube.
    let o = (c - mouth).normalize_or(out);
    push_fixture(
        plan,
        FixtureKind::ExitSign,
        DVec3::new(mouth.x, floor + TUBE_HEIGHT - 0.55, mouth.y)
            + DVec3::new(o.x, 0.0, o.y) * (TUBE_HALF - 0.5),
        dir,
        DVec3::new(0.7, 0.32, 0.05),
        0,
    );
    lamp(
        plan,
        DVec3::new(mouth.x, floor + 2.4, mouth.y) + DVec3::new(o.x, 0.0, o.y) * 1.8,
        [0.4, 1.0, 0.55],
        4.0,
        9.0,
    );
}
