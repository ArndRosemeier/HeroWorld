//! A gang's den ([`Scheme::Den`]): a house built to be fought through,
//! not lived in.
//!
//! One great hall rises through the whole height of the house, across its
//! full width. Balconies hang at its two ends, at every storey and at
//! levels in between, each as deep as the dice say; open flights of
//! stairs climb along the side walls from a balcony at one end to the
//! next one up at the other, so the way to the top crosses the hall again
//! and again; bridges cross it between balconies of one level. The rooms
//! stand in front of the hall, behind it or both: every storey its own
//! height, one room or two, a room now and then two storeys high behind
//! a wall of glass, another open to the hall like a box at the theatre.
//!
//! The storeys' floors run level through the house (the facade's rows of
//! windows fit them); everything at other heights hangs in the hall.

use super::office::{levels, rect, snap};
use super::stairs::risers;
use super::{
    Builder, Flight, Layout, LinkKind, Scheme, SpaceKind, Stair, StairKind, ARCH, DOOR, DOUBLE,
    HEADROOM, RISE, TREAD,
};
use glam::{DVec2, DVec3};
use hw_core::{Rng, Seed};

/// The least plot a den fits on (m).
pub const MIN_DEN: DVec2 = DVec2::new(12.0, 22.0);
/// Width of a flight and of the lane it climbs in, along a side wall (m).
const LANE: f64 = 2.6;
/// The least depth of a balcony (m).
const DECK: f64 = 3.0;
/// The least climb from one balcony to the next (m): room to stand under
/// the upper one.
const STEP: f64 = 3.0;
/// Clear height kept over a flight's steps under a balcony (m).
const CLEAR: f64 = HEADROOM + 0.1;
/// The shortest hall (m).
const HALL: f64 = 15.0;
/// Between the lanes and a bridge (m).
const BESIDE: f64 = 1.0;

/// An end of the hall: towards the street, or away from it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum End {
    Front,
    Back,
}

impl End {
    fn other(self) -> Self {
        match self {
            End::Front => End::Back,
            End::Back => End::Front,
        }
    }
}

/// A flight as the balconies see it: the end it leaves from, how far from
/// that end's wall its foot is and at what height, and how far from the
/// other end's wall its head is, and how high.
struct Run {
    from: End,
    foot: f64,
    y0: f64,
    head: f64,
    y1: f64,
}

/// How deep a balcony at `end`, `y` high, may be so that nobody on the
/// flights so far bumps their head on it and none of them is in its way.
fn cap(runs: &[Run], end: End, y: f64) -> f64 {
    let mut c = f64::INFINITY;
    for r in runs {
        if r.from == end {
            // Over the flight's foot: as far out as its steps are still
            // well below.
            if y > r.y0 + 1e-6 {
                c = c.min(r.foot + (y - r.y0 - CLEAR).max(0.0) * (TREAD / RISE));
            }
        } else if (y > r.y1 + 1e-6 && y - r.y1 < CLEAR) || (y > r.y0 - 0.05 && y < r.y1 - 1e-6) {
            // Close over the flight's head, or under its last steps.
            c = c.min(r.head);
        }
    }
    c
}

/// The rooms in front of the hall or behind it.
struct Zone {
    end: End,
    z0: f64,
    z1: f64,
    /// Per storey: the rooms whose floor is there, and whether each is
    /// two storeys high.
    rooms: Vec<Vec<(usize, bool)>>,
}

/// A room's kind by the dice.
fn room_kind(rng: &mut Rng) -> SpaceKind {
    *rng.pick(&[
        SpaceKind::Living,
        SpaceKind::Service,
        SpaceKind::Bedroom,
        SpaceKind::Shop,
        SpaceKind::Kitchen,
        SpaceKind::Living,
        SpaceKind::Service,
    ])
}

/// Builds a zone's rooms, storey by storey: one room across the house,
/// two side by side, or two of which one is two storeys high (the room
/// over the other looks down into it through glass).
fn rooms(b: &mut Builder, rng: &mut Rng, zone: &mut Zone, w: f64, ys: &[f64], shop: bool) {
    let n = ys.len() - 1;
    let (z0, z1) = (zone.z0, zone.z1);
    // A tall room from the storey below: where the house is split, which
    // half the tall room is, and the room itself.
    let mut tall: Option<(f64, bool, usize)> = None;
    for i in 0..n {
        let (y, h) = (ys[i], ys[i + 1] - ys[i]);
        if let Some((sx, left, below)) = tall.take() {
            let r = if left {
                rect(sx, z0, w, z1)
            } else {
                rect(0.0, z0, sx, z1)
            };
            let room = b.space(room_kind(rng), r, y, h);
            b.edge(room, below, LinkKind::Glass, y);
            zone.rooms.push(vec![(room, false)]);
            continue;
        }
        if i == 0 && shop {
            let s = b.space(SpaceKind::Shop, rect(0.0, z0, w, z1), y, h);
            b.entrance(s, snap(w * 0.5), DOUBLE);
            zone.rooms.push(vec![(s, false)]);
            continue;
        }
        let dice = rng.index(10);
        // The top storey keeps one big room (the boss's).
        if dice < 4 || (i + 1 == n && dice < 7) {
            let s = b.space(room_kind(rng), rect(0.0, z0, w, z1), y, h);
            zone.rooms.push(vec![(s, false)]);
            continue;
        }
        let sx = snap(w * *rng.pick(&[0.4, 0.5, 0.6])).clamp(5.0, w - 5.0);
        let left_tall = rng.chance(0.5);
        let high = dice >= 7 && i + 1 < n;
        let mut pair = Vec::new();
        for (left, r) in [(true, rect(0.0, z0, sx, z1)), (false, rect(sx, z0, w, z1))] {
            let up = high && left == left_tall;
            let hh = if up { ys[i + 2] - y } else { h };
            let s = b.space(room_kind(rng), r, y, hh);
            if up {
                tall = Some((sx, left, s));
            }
            pair.push((s, up));
        }
        b.door(pair[0].0, pair[1].0, LinkKind::Door, y, DOOR);
        zone.rooms.push(pair);
    }
}

/// The way from the hall's floor or a balcony (`from`) into `room` at
/// `y`: a door, a pair of doors, a door beside a wall of glass (always
/// so for a `tall` room), or no wall at all.
fn way_in(b: &mut Builder, rng: &mut Rng, from: usize, room: usize, y: f64, tall: bool) {
    let Some((_, _, lo, hi)) = b.shared(from, room) else {
        return;
    };
    let len = hi - lo;
    let dice = if tall { 3 } else { rng.index(5) };
    match dice {
        3 | 4 if len >= 5.5 => {
            // The door at one side, glass along the rest.
            let door_left = rng.chance(0.5);
            let (c, g0, g1) = if door_left {
                (lo + 1.9, lo + 3.6, hi - 0.4)
            } else {
                (hi - 1.9, lo + 0.4, hi - 3.6)
            };
            b.door_at(from, room, LinkKind::Door, y, DOOR, Some(c));
            b.edge_part(from, room, LinkKind::Glass, y, g0, g1);
        }
        2 if len >= 6.0 => b.edge_part(from, room, LinkKind::Open, y, lo + 0.4, hi - 0.4),
        1 if len >= DOUBLE + 1.0 => {
            let c = rng.range(lo + DOUBLE * 0.5 + 0.5, hi - DOUBLE * 0.5 - 0.5);
            b.door_at(from, room, LinkKind::Double, y, DOUBLE, Some(snap(c)));
        }
        _ => {
            let c = rng.range(lo + DOOR * 0.5 + 0.4, hi - DOOR * 0.5 - 0.4);
            b.door_at(from, room, LinkKind::Door, y, DOOR, Some(snap(c)));
        }
    }
}

/// Plans a den on a plot of at least [`MIN_DEN`] with `storeys` storeys.
pub fn plan(seed: Seed, size: DVec2, storeys: u32) -> Option<Layout> {
    let (w, d) = (size.x, size.y);
    let n = storeys as usize;
    if w < MIN_DEN.x || d < MIN_DEN.y || n < 2 {
        return None;
    }
    let mut rng = seed.child("den").rng();
    let depth = |rng: &mut Rng| -> f64 { *rng.pick(&[5.5, 6.0, 6.5, 7.0]) };

    // Where the hall stands: at the street, at the back, or in the middle
    // (on a plot deep enough for rooms either side of it).
    let middle = d >= HALL + 12.0 && rng.chance(0.75);
    let (zf, zb) = if middle {
        let zf = depth(&mut rng).min(d - HALL - 5.5);
        let fits: Vec<f64> = [5.5, 6.0, 6.5, 7.0, 8.0]
            .into_iter()
            .filter(|z| zf + z <= d - HALL)
            .collect();
        let zb = *rng.pick(&fits);
        // A very deep plot: deeper rooms rather than an endless hall.
        let extra = snap(((d - zf - zb - 21.0) * 0.5).max(0.0));
        (zf + extra, zb + extra)
    } else {
        let z = depth(&mut rng).min(d - HALL).max(snap(d - 23.0).min(9.0));
        if rng.chance(0.5) {
            (z, 0.0)
        } else {
            (0.0, z)
        }
    };
    let (hz0, hz1) = (zf, d - zb);
    let long = hz1 - hz0;
    if long < HALL - 1e-6 {
        return None;
    }

    // The highest one flight climbs: its run fits between two balconies.
    let most = (((long - 2.0 * DECK) / TREAD).floor() + 1.0) * RISE;
    // Storey heights: none like the one below; those too high for one
    // flight are climbed in two, by a balcony in between.
    let fits = |h: f64| h <= most + 1e-9 || (h >= 2.0 * STEP && h <= 2.0 * most);
    let mut hs: Vec<f64> = Vec::new();
    for i in 0..n {
        let all = [4.0, 4.5, 5.0, 5.5, 6.0, 6.5, 7.0, 7.5];
        let ok: Vec<f64> = all
            .into_iter()
            .filter(|h| fits(*h) && (i > 0 || *h >= 5.0))
            .collect();
        let unlike: Vec<f64> = ok
            .iter()
            .copied()
            .filter(|h| hs.last().is_none_or(|p| (h - p).abs() >= 1.0))
            .collect();
        hs.push(*rng.pick(if unlike.is_empty() { &ok } else { &unlike }));
    }
    let ys = levels(&hs);
    let top = ys[n];

    // The stations of the climb: (height, the storey whose floor it is).
    let mut stations: Vec<(f64, Option<usize>)> = vec![(0.0, Some(0))];
    for i in 0..n - 1 {
        let h = hs[i];
        let split = h > most + 1e-9 || (h >= 2.0 * STEP && rng.chance(0.85));
        if split {
            let (lo, hi) = (STEP.max(h - most), most.min(h - STEP));
            let p = lo + (hi - lo) * *rng.pick(&[0.0, 0.5, 1.0]);
            stations.push((ys[i] + p, None));
        }
        stations.push((ys[i + 1], Some(i + 1)));
    }

    let mut b = Builder::default();
    let hall = b.space(SpaceKind::Hall, rect(0.0, hz0, w, hz1), 0.0, top);
    let mut zones: Vec<Zone> = Vec::new();
    if zf > 0.0 {
        zones.push(Zone {
            end: End::Front,
            z0: 0.0,
            z1: zf,
            rooms: Vec::new(),
        });
    }
    if zb > 0.0 {
        zones.push(Zone {
            end: End::Back,
            z0: hz1,
            z1: d,
            rooms: Vec::new(),
        });
    }
    for zone in &mut zones {
        let shop = zone.end == End::Front;
        rooms(&mut b, &mut rng, zone, w, &ys, shop);
    }
    if zf == 0.0 {
        b.entrance(hall, snap(w * 0.5), DOUBLE);
    }
    // The ground floor: straight off the hall's floor.
    for zone in &zones {
        for &(room, tall) in &zone.rooms[0] {
            if b.spaces[room].kind == SpaceKind::Shop && zone.end == End::Front {
                let wide = ARCH.min(w - 2.0 * LANE - 1.0);
                b.door_at(hall, room, LinkKind::Arch, 0.0, wide, Some(w * 0.5));
            } else {
                way_in(&mut b, &mut rng, hall, room, 0.0, tall);
            }
        }
    }

    // A balcony `deep` at `end`, `y` high, with the ways into the rooms
    // behind it if it is a storey's floor.
    let z_of = |end: End, off: f64| match end {
        End::Front => hz0 + off,
        End::Back => hz1 - off,
    };
    let deck = |b: &mut Builder, rng: &mut Rng, end: End, y: f64, deep: f64, storey| {
        let (a, c) = (z_of(end, 0.0), z_of(end, deep));
        let s = b.nested(
            hall,
            SpaceKind::Landing,
            rect(0.0, a.min(c), w, a.max(c)),
            y,
            HEADROOM,
        );
        if let (Some(i), Some(zone)) = (storey, zones.iter().find(|z| z.end == end)) {
            let i: usize = i;
            for &(room, tall) in &zone.rooms[i] {
                way_in(b, rng, s, room, y, tall);
            }
        }
        s
    };
    // Is a balcony wanted at `end` of station `k` though no flight ends
    // there? Where rooms are behind it; else now and then, as a lookout
    // (never low over the way in from the street).
    let wanted = |rng: &mut Rng, end: End, (y, storey): (f64, Option<usize>)| {
        let rooms = storey.is_some() && zones.iter().any(|z| z.end == end);
        let over_door = end == End::Front && zf == 0.0 && y < 5.5;
        rooms || (!over_door && rng.chance(0.5))
    };
    // A bridge between the two balconies of one level.
    let bridge = |b: &mut Builder, rng: &mut Rng, y: f64, front: f64, back: f64| {
        let (lo, hi) = (LANE + BESIDE, w - LANE - BESIDE);
        let wide = if rng.chance(0.2) {
            hi - lo
        } else {
            *rng.pick(&[2.6, 3.0, 4.0])
        };
        let x0 = lo + ((hi - lo - wide) * *rng.pick(&[0.0, 0.5, 1.0]) * 10.0).round() / 10.0;
        let kind = if wide >= 5.0 {
            SpaceKind::Bridge
        } else {
            SpaceKind::Landing
        };
        b.nested(
            hall,
            kind,
            rect(x0, hz0 + front, x0 + wide, hz1 - back),
            y,
            HEADROOM,
        );
    };

    let mut runs: Vec<Run> = Vec::new();
    let mut flights: Vec<Flight> = Vec::new();
    // The first flight leaves the street end where the hall is at the
    // street (so no balcony hangs low over the way in).
    let mut end = if zf == 0.0 || rng.chance(0.5) {
        End::Front
    } else {
        End::Back
    };
    let mut left = rng.chance(0.5);
    // How deep the balcony is the next flight leaves from.
    let mut here = 0.0;
    let climbs = stations.len() - 1;
    for k in 0..climbs {
        let (y0, y1) = (stations[k].0, stations[k + 1].0);
        let (r, rise) = risers(y1 - y0);
        let run = Flight::run(r);
        let to = end.other();
        // What the flight after this one needs of the balcony it leaves.
        let next = if k + 1 < climbs {
            long - DECK - Flight::run(risers(stations[k + 2].0 - y1).0)
        } else {
            f64::INFINITY
        };
        let limit = cap(&runs, to, y1).min(next);
        let options = |most: f64| -> Vec<f64> {
            [3.0, 3.5, 4.0, 5.0]
                .into_iter()
                .filter(|v| *v <= most + 1e-9)
                .collect()
        };
        // Where it starts and where it ends, from their walls.
        let (foot, head, deep) = if k == 0 {
            // From the hall's floor, straight to the balcony's edge.
            let fits = options(limit.min(long - run - DECK));
            let deep = *rng.pick(&fits);
            (long - deep - run, deep, deep)
        } else {
            let head = long - here - run;
            let fits = options(limit.min(head));
            let mut deep = *rng.pick(&fits);
            // Rather a deeper balcony than a stub of a landing.
            if head - deep < 1.0 && head <= limit {
                deep = head;
            }
            (here, head, deep)
        };
        let x = if left { LANE * 0.5 } else { w - LANE * 0.5 };
        let dir = match end {
            End::Front => DVec2::Y,
            End::Back => -DVec2::Y,
        };
        flights.push(Flight::Straight {
            from: DVec3::new(x, y0, z_of(end, foot)),
            dir,
            width: LANE,
            risers: r,
            rise,
        });
        runs.push(Run {
            from: end,
            foot,
            y0,
            head,
            y1,
        });
        // The balcony across from the one this flight leaves, and the
        // bridge between them.
        if k > 0 && wanted(&mut rng, to, stations[k]) {
            let fits = options(cap(&runs, to, y0));
            let deep = *rng.pick(&fits);
            deck(&mut b, &mut rng, to, y0, deep, stations[k].1);
            let (f, bk) = if to == End::Front {
                (deep, here)
            } else {
                (here, deep)
            };
            bridge(&mut b, &mut rng, y0, f, bk);
        }
        // The balcony it arrives at, and the landing out to its head.
        deck(&mut b, &mut rng, to, y1, deep, stations[k + 1].1);
        if head - deep > 0.05 {
            let (a, c) = (z_of(to, deep), z_of(to, head));
            let (x0, x1) = if left { (0.0, LANE) } else { (w - LANE, w) };
            b.nested(
                hall,
                SpaceKind::Landing,
                rect(x0, a.min(c), x1, a.max(c)),
                y1,
                HEADROOM,
            );
        }
        here = deep;
        end = to;
        left = !left;
    }
    // Across from the last balcony.
    let last = stations[climbs];
    if climbs > 0 && wanted(&mut rng, end.other(), last) {
        let to = end.other();
        let fits: Vec<f64> = [3.0, 3.5, 4.0, 5.0]
            .into_iter()
            .filter(|v| *v <= cap(&runs, to, last.0) + 1e-9)
            .collect();
        let deep = *rng.pick(&fits);
        deck(&mut b, &mut rng, to, last.0, deep, last.1);
        let (f, bk) = if to == End::Front {
            (deep, here)
        } else {
            (here, deep)
        };
        bridge(&mut b, &mut rng, last.0, f, bk);
    }
    b.stairs.push(Stair {
        kind: StairKind::Straight,
        host: hall,
        flights,
        landings: Vec::new(),
    });
    Some(b.finish(Scheme::Den, size))
}
