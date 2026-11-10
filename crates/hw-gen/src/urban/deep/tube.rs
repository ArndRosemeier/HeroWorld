//! The Aether Tube: a pneumatic capsule line under every main road, with
//! stations beside it.
//!
//! The network ([`TubeNet`]) is planned once from the main roads: cheap
//! polylines and floor heights, so it reaches across a megacity without
//! planning any street. A tile turns the stretches and stations it holds
//! into voids, gates, lifts and fittings.

use super::{
    name, overlaps, room, run, tile_of, DeepEntrance, DeepLamp, EntranceKind, Fixture, FixtureKind,
    Plan, StationHall, TileKey,
};
use crate::urban::underground::{Underground, Void, VoidKind, SHAFT_HALF};
use crate::urban::world::{TownWorld, DECK};
use crate::water::segment_distance;
use glam::{DVec2, DVec3};
use std::collections::{BTreeMap, BinaryHeap};

/// Half the width and the height of the walkway inside the tube.
pub const TUBE_HALF: f64 = 1.6;
pub const TUBE_HEIGHT: f64 = 2.9;
/// Tube floor below the lowest ground near the line.
pub const TUBE_DEPTH: f64 = 16.0;
/// Points along a line (m).
const SAMPLE: f64 = 20.0;
/// Steepest the tube climbs or falls.
const GRADE: f64 = 0.1;
/// Between stations along a line, and between any two stations.
const STATION_EVERY: f64 = 620.0;
const STATION_GAP: f64 = 320.0;
/// A station hall: half length, half width, height; its centre this far
/// off the line.
pub const HALL_HALF: DVec2 = DVec2::new(26.0, 7.5);
pub const HALL_HEIGHT: f64 = 6.5;
pub const HALL_OFF: f64 = TUBE_HALF + 3.0 + HALL_HALF.y;
/// Boarding gates along the hall (local x).
pub const GATES: [f64; 2] = [-11.0, 11.0];

/// One line under one main road.
#[derive(Clone, Debug, PartialEq)]
pub struct TubeLine {
    pub pts: Vec<DVec2>,
    /// Walkway floor at each point.
    pub floors: Vec<f64>,
    /// The road's half width plus pavement (for station entrances).
    pub verge: f64,
    /// Metres along the line at each point.
    pub arcs: Vec<f64>,
}

/// A station on the network.
#[derive(Clone, Debug, PartialEq)]
pub struct TubeStation {
    pub name: String,
    pub line: usize,
    /// Index of its point on the line.
    pub at: usize,
    pub pos: DVec2,
    /// The line's way at the station.
    pub dir: DVec2,
    pub floor: f64,
    /// The hall's side of the line (+1: `dir.perp()`).
    pub side: f64,
    /// Has a lift down to the Deeps.
    pub deep_lift: bool,
}

impl TubeStation {
    /// The hall's centre in plan.
    pub fn hall_centre(&self) -> DVec2 {
        self.pos + self.dir.perp() * self.side * HALL_OFF
    }

    /// The hall's void.
    pub fn hall(&self) -> Void {
        room(
            VoidKind::Station,
            self.hall_centre(),
            self.dir,
            HALL_HALF,
            self.floor,
            HALL_HEIGHT,
        )
    }

    /// Where the lift down to the Deeps stands (if it has one).
    pub fn deep_lift_pos(&self) -> DVec2 {
        self.hall().world(DVec2::new(
            -(HALL_HALF.x - 3.5),
            self.side * (HALL_HALF.y - 3.0),
        ))
    }
}

/// The whole network.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TubeNet {
    pub lines: Vec<TubeLine>,
    pub stations: Vec<TubeStation>,
    /// Stretches (line, index of the first point) by the tile of their
    /// middle.
    stretches: BTreeMap<TileKey, Vec<(u32, u32)>>,
    by_tile: BTreeMap<TileKey, Vec<usize>>,
    joins: Vec<Join>,
}

/// Floor for a tube at `p`: well under the lowest ground round about
/// (river beds too).
fn base_floor(w: &TownWorld, p: DVec2) -> f64 {
    let mut low = w.ground(p);
    for r in [20.0, 40.0] {
        for k in 0..8 {
            let a = std::f64::consts::TAU * k as f64 / 8.0;
            low = low.min(w.ground(p + hw_core::det::dir2(a) * r));
        }
    }
    low - TUBE_DEPTH
}

/// Points every [`SAMPLE`] metres or less along `pts`, ends kept, and the
/// points in `cuts` (on the line) added where they fall.
fn resample(pts: &[DVec2], cuts: &[DVec2]) -> Vec<DVec2> {
    let mut out = vec![pts[0]];
    for s in pts.windows(2) {
        let (a, b) = (s[0], s[1]);
        let len = a.distance(b);
        if len < 1e-6 {
            continue;
        }
        let dir = (b - a) / len;
        // Stops along this segment: its end and every cut on it.
        let mut stops: Vec<f64> = cuts
            .iter()
            .filter(|c| segment_distance(**c, a, b) < 0.01)
            .map(|c| (*c - a).dot(dir))
            .filter(|t| *t > 0.5 && *t < len - 0.5)
            .collect();
        stops.push(len);
        stops.sort_by(f64::total_cmp);
        let mut t0 = 0.0;
        for t1 in stops {
            let n = ((t1 - t0) / SAMPLE).ceil().max(1.0) as usize;
            for k in 1..=n {
                out.push(a + dir * (t0 + (t1 - t0) * k as f64 / n as f64));
            }
            t0 = t1;
        }
    }
    out.dedup_by(|a, b| a.distance(*b) < 0.5);
    out
}

/// Where lines meet: a line's end joins the nearest point of another line
/// within a few metres (an end, or a point in its middle at a T).
type Join = ((usize, usize), (usize, usize));

/// Line ends that land on other roads, as cut points on those roads.
fn junction_cuts(roads: &[&[DVec2]]) -> Vec<Vec<DVec2>> {
    const CELL: f64 = 64.0;
    let key = |p: DVec2| ((p.x / CELL).floor() as i64, (p.y / CELL).floor() as i64);
    let mut grid: BTreeMap<(i64, i64), Vec<(usize, usize)>> = BTreeMap::new();
    for (ri, pts) in roads.iter().enumerate() {
        for i in 0..pts.len().saturating_sub(1) {
            let (a, b) = (pts[i], pts[i + 1]);
            let (lo, hi) = (a.min(b) - DVec2::splat(6.0), a.max(b) + DVec2::splat(6.0));
            let (k0, k1) = (key(lo), key(hi));
            for x in k0.0..=k1.0 {
                for y in k0.1..=k1.1 {
                    grid.entry((x, y)).or_default().push((ri, i));
                }
            }
        }
    }
    let mut cuts = vec![Vec::new(); roads.len()];
    for (ri, pts) in roads.iter().enumerate() {
        for e in [pts[0], pts[pts.len() - 1]] {
            for &(rj, i) in grid.get(&key(e)).into_iter().flatten() {
                if rj == ri {
                    continue;
                }
                let (a, b) = (roads[rj][i], roads[rj][i + 1]);
                if segment_distance(e, a, b) < 5.0 {
                    let ab = b - a;
                    let t = ((e - a).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
                    cuts[rj].push(a + ab * t);
                }
            }
        }
    }
    cuts
}

/// Pairs of points where lines meet.
fn joins(lines: &[TubeLine]) -> Vec<Join> {
    const CELL: f64 = 64.0;
    let key = |p: DVec2| ((p.x / CELL).floor() as i64, (p.y / CELL).floor() as i64);
    let mut grid: BTreeMap<(i64, i64), Vec<(usize, usize)>> = BTreeMap::new();
    for (li, l) in lines.iter().enumerate() {
        for (i, p) in l.pts.iter().enumerate() {
            grid.entry(key(*p)).or_default().push((li, i));
        }
    }
    let mut out = Vec::new();
    for (li, l) in lines.iter().enumerate() {
        for e in [0, l.pts.len() - 1] {
            let p = l.pts[e];
            let k = key(p);
            let mut best: BTreeMap<usize, (f64, usize)> = BTreeMap::new();
            for x in k.0 - 1..=k.0 + 1 {
                for y in k.1 - 1..=k.1 + 1 {
                    for &(lj, j) in grid.get(&(x, y)).into_iter().flatten() {
                        if lj == li {
                            continue;
                        }
                        let d = lines[lj].pts[j].distance(p);
                        if d < 5.5 && best.get(&lj).is_none_or(|b| d < b.0) {
                            best.insert(lj, (d, j));
                        }
                    }
                }
            }
            for (lj, (_, j)) in best {
                out.push(((li, e), (lj, j)));
            }
        }
    }
    out
}

/// Lowers floors so the walkway keeps clear of the shallow underground.
fn clear_shallow(u: &Underground, p: DVec2, f: f64) -> f64 {
    let mut f = f;
    // A few rounds: dropping under one void may meet another.
    for _ in 0..3 {
        let probe = Void {
            kind: VoidKind::Tube,
            origin: p - DVec2::splat(TUBE_HALF),
            along: DVec2::X,
            min: DVec2::ZERO,
            max: DVec2::splat(TUBE_HALF * 2.0),
            floor: [f, f],
            height: TUBE_HEIGHT,
            open: false,
        };
        let mut low = f;
        for i in u.near(p, TUBE_HALF + 4.0) {
            let v = &u.voids[i];
            if overlaps(&probe, v, 1.5) {
                low = low.min(v.floor[0].min(v.floor[1]) - 2.5 - TUBE_HEIGHT);
            }
        }
        if low >= f {
            break;
        }
        f = low;
    }
    f
}

impl TubeNet {
    pub fn plan(w: &TownWorld) -> TubeNet {
        let u = w.core_underground();
        let roads: Vec<_> = super::super::underground::main_roads(w)
            .filter(|r| r.pts.len() >= 2)
            .collect();
        let cuts = junction_cuts(&roads.iter().map(|r| r.pts.as_slice()).collect::<Vec<_>>());
        let mut lines: Vec<TubeLine> = Vec::new();
        for (r, cuts) in roads.iter().zip(&cuts) {
            let pts = resample(&r.pts, cuts);
            let floors = pts
                .iter()
                .map(|&p| clear_shallow(u, p, base_floor(w, p)))
                .collect();
            lines.push(TubeLine {
                arcs: super::nooks::arcs(&pts),
                pts,
                floors,
                verge: r.half + r.pavement,
            });
        }
        // Between the samples too: a stretch can clip a void (a corner of
        // Molehaven) that neither of its ends' probes meets.
        for l in &mut lines {
            for _ in 0..3 {
                let mut moved = false;
                for i in 0..l.pts.len().saturating_sub(1) {
                    let (a, b) = (l.pts[i], l.pts[i + 1]);
                    let Some(v) = run(
                        VoidKind::Tube,
                        a,
                        b,
                        TUBE_HALF,
                        l.floors[i],
                        l.floors[i + 1],
                        TUBE_HEIGHT,
                    ) else {
                        continue;
                    };
                    let mid = (a + b) * 0.5;
                    for j in u.near(mid, a.distance(b) * 0.5 + TUBE_HALF + 4.0) {
                        let o = &u.voids[j];
                        if overlaps(&v, o, 1.5) {
                            let low = o.floor[0].min(o.floor[1]) - 2.5 - TUBE_HEIGHT;
                            for k in [i, i + 1] {
                                if l.floors[k] > low {
                                    l.floors[k] = low;
                                    moved = true;
                                }
                            }
                        }
                    }
                }
                if !moved {
                    break;
                }
            }
        }
        let joins = joins(&lines);
        // Gentle grades, and lines that meet share their floor there.
        for _ in 0..6 {
            for l in &mut lines {
                let n = l.pts.len();
                for i in 1..n {
                    let d = l.pts[i].distance(l.pts[i - 1]);
                    l.floors[i] = l.floors[i].min(l.floors[i - 1] + GRADE * d);
                }
                for i in (0..n - 1).rev() {
                    let d = l.pts[i].distance(l.pts[i + 1]);
                    l.floors[i] = l.floors[i].min(l.floors[i + 1] + GRADE * d);
                }
            }
            for &((a, i), (b, j)) in &joins {
                let f = lines[a].floors[i].min(lines[b].floors[j]);
                lines[a].floors[i] = f;
                lines[b].floors[j] = f;
            }
        }
        let mut net = TubeNet {
            lines,
            joins,
            ..TubeNet::default()
        };
        for (li, l) in net.lines.iter().enumerate() {
            for i in 0..l.pts.len() - 1 {
                let m = (l.pts[i] + l.pts[i + 1]) * 0.5;
                net.stretches
                    .entry(tile_of(m))
                    .or_default()
                    .push((li as u32, i as u32));
            }
        }
        net.place_stations(w);
        for (i, s) in net.stations.iter().enumerate() {
            net.by_tile.entry(tile_of(s.pos)).or_default().push(i);
        }
        net
    }

    fn place_stations(&mut self, w: &TownWorld) {
        const FIRST: &[&str] = &[
            "Rocket",
            "Atom",
            "Comet",
            "Orbit",
            "Starlight",
            "Jetstream",
            "Nova",
            "Saturn",
            "Zephyr",
            "Meteor",
            "Galaxy",
            "Radium",
            "Neon",
            "Chrome",
            "Aether",
            "Telstar",
            "Cosmo",
            "Satellite",
            "Polaris",
            "Mercury",
        ];
        const SECOND: &[&str] = &[
            "Row",
            "Square",
            "Circus",
            "Gate",
            "Park",
            "Plaza",
            "Cross",
            "Terrace",
            "Heights",
            "Market",
            "Junction",
            "Parade",
            "Lane",
            "Court",
            "Halls",
            "Gardens",
            "Arcade",
            "Boulevard",
        ];
        let mut names: std::collections::BTreeSet<String> = Default::default();
        let mut stations: Vec<TubeStation> = Vec::new();
        for (li, l) in self.lines.iter().enumerate() {
            let n = l.pts.len();
            let mut s = 0.0;
            let mut since = STATION_EVERY * 0.6;
            let total: f64 = l.pts.windows(2).map(|q| q[0].distance(q[1])).sum();
            for i in 1..n {
                let d = l.pts[i].distance(l.pts[i - 1]);
                s += d;
                since += d;
                if since < STATION_EVERY || s < 60.0 || total - s < 60.0 {
                    continue;
                }
                let p = l.pts[i];
                // Straight for the hall's length either way.
                let back = l.pts[i.saturating_sub(2)];
                let fwd = l.pts[(i + 2).min(n - 1)];
                let (d0, d1) = (
                    (p - back).normalize_or_zero(),
                    (fwd - p).normalize_or_zero(),
                );
                if d0.dot(d1) < 0.97 || p.distance(back) < 30.0 || p.distance(fwd) < 30.0 {
                    continue;
                }
                let dir = (fwd - back).normalize_or(DVec2::X);
                if stations.iter().any(|o| o.pos.distance(p) < STATION_GAP) {
                    continue;
                }
                let dry = [-30.0, 0.0, 30.0]
                    .iter()
                    .all(|&t| w.ground(p + dir * t) > DECK + 1.0);
                if !dry {
                    continue;
                }
                let k = stations.len();
                let seed = w.seed.child("tube-station").child(k);
                let mut r = seed.rng();
                let side = if r.chance(0.5) { 1.0 } else { -1.0 };
                let mut st = TubeStation {
                    name: String::new(),
                    line: li,
                    at: i,
                    pos: p,
                    dir,
                    floor: l.floors[i],
                    side,
                    deep_lift: r.chance(0.45),
                };
                // The hall must keep clear of the shallow underground and of
                // other lines; try the other side before giving up.
                let ok = |st: &TubeStation, net: &TubeNet| {
                    let hall = st.hall();
                    let u = w.core_underground();
                    let shallow = u
                        .near(st.hall_centre(), 40.0)
                        .into_iter()
                        .all(|j| !overlaps(&hall, &u.voids[j], 1.5));
                    shallow
                        && !net.stretch_voids_near(st.hall_centre(), 45.0).iter().any(
                            |(ol, oi, v)| {
                                !(*ol == li && oi.abs_diff(i) <= 4) && overlaps(&hall, v, 2.0)
                            },
                        )
                };
                if !ok(&st, self) {
                    st.side = -st.side;
                    if !ok(&st, self) {
                        continue;
                    }
                }
                let mut nm = name(seed.child("name"), FIRST, SECOND);
                let mut tries = 0;
                while names.contains(&nm) && tries < 20 {
                    tries += 1;
                    nm = name(seed.child("name").child(tries), FIRST, SECOND);
                }
                if names.contains(&nm) {
                    nm = format!("{nm} {}", k + 1);
                }
                names.insert(nm.clone());
                st.name = nm;
                stations.push(st);
                since = 0.0;
            }
        }
        self.stations = stations;
    }

    /// A stretch's void.
    pub fn stretch_void(&self, line: usize, i: usize) -> Option<Void> {
        let l = &self.lines[line];
        run(
            VoidKind::Tube,
            l.pts[i],
            l.pts[i + 1],
            TUBE_HALF,
            l.floors[i],
            l.floors[i + 1],
            TUBE_HEIGHT,
        )
    }

    /// Stretches (line, point, void) whose middle is within `r` of `p`
    /// (roughly: by tile).
    pub fn stretch_voids_near(&self, p: DVec2, r: f64) -> Vec<(usize, usize, Void)> {
        let (a, b) = (
            tile_of(p - DVec2::splat(r + SAMPLE)),
            tile_of(p + DVec2::splat(r + SAMPLE)),
        );
        let mut out = Vec::new();
        for x in a.0..=b.0 {
            for y in a.1..=b.1 {
                for &(l, i) in self.stretches.get(&(x, y)).into_iter().flatten() {
                    let (l, i) = (l as usize, i as usize);
                    let m = (self.lines[l].pts[i] + self.lines[l].pts[i + 1]) * 0.5;
                    if m.distance(p) <= r + SAMPLE {
                        if let Some(v) = self.stretch_void(l, i) {
                            out.push((l, i, v));
                        }
                    }
                }
            }
        }
        out
    }

    /// Is point `i` of line `l` a dead end (a line end no other line meets)?
    pub fn is_end(&self, l: usize, i: usize) -> bool {
        let last = self.lines[l].pts.len() - 1;
        (i == 0 || i == last) && !self.joins.iter().any(|&(a, b)| a == (l, i) || b == (l, i))
    }

    /// Stretches (line, first point) filed in tile `k` (by their middle).
    pub fn stretches_in(&self, k: TileKey) -> Option<&[(u32, u32)]> {
        self.stretches.get(&k).map(|v| v.as_slice())
    }

    /// Stations whose platform is in tile `k`.
    pub fn stations_in(&self, k: TileKey) -> &[usize] {
        self.by_tile.get(&k).map_or(&[], |v| v.as_slice())
    }

    /// Is any station within `r` of `p` (plan)? Cheap: by tile.
    pub fn station_within(&self, p: DVec2, r: f64) -> bool {
        let (a, b) = (tile_of(p - DVec2::splat(r)), tile_of(p + DVec2::splat(r)));
        (a.0..=b.0).any(|x| {
            (a.1..=b.1).any(|y| {
                self.stations_in((x, y))
                    .iter()
                    .any(|&i| self.stations[i].pos.distance(p) <= r)
            })
        })
    }

    /// The station nearest `p` within `r`.
    pub fn station_near(&self, p: DVec2, r: f64) -> Option<usize> {
        self.stations
            .iter()
            .enumerate()
            .map(|(i, s)| (i, s.hall_centre().distance(p).min(s.pos.distance(p))))
            .filter(|x| x.1 <= r)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|x| x.0)
    }

    /// Where a capsule stands at station `s`: the middle of the walkway at
    /// the station's point, and the line's way there.
    pub fn platform(&self, s: usize) -> (DVec3, DVec2) {
        let st = &self.stations[s];
        (DVec3::new(st.pos.x, st.floor, st.pos.y), st.dir)
    }

    /// The way through the tubes from station `a` to station `b`: points on
    /// the walkway floor, along the lines and across junctions. `None` when
    /// the lines do not join up.
    pub fn path(&self, a: usize, b: usize) -> Option<Vec<DVec3>> {
        let (sa, sb) = (self.stations.get(a)?, self.stations.get(b)?);
        // Nodes: (line, point); junctions join them across lines.
        let mut links: BTreeMap<(usize, usize), Vec<(usize, usize)>> = BTreeMap::new();
        for &(a, b) in &self.joins {
            links.entry(a).or_default().push(b);
            links.entry(b).or_default().push(a);
        }
        let start = (sa.line, sa.at);
        let goal = (sb.line, sb.at);
        let mut dist: BTreeMap<(usize, usize), f64> = BTreeMap::new();
        let mut prev: BTreeMap<(usize, usize), (usize, usize)> = BTreeMap::new();
        #[derive(PartialEq)]
        struct Item(f64, (usize, usize));
        impl Eq for Item {}
        impl PartialOrd for Item {
            fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(o))
            }
        }
        impl Ord for Item {
            fn cmp(&self, o: &Self) -> std::cmp::Ordering {
                o.0.total_cmp(&self.0).then_with(|| o.1.cmp(&self.1))
            }
        }
        let mut heap = BinaryHeap::new();
        dist.insert(start, 0.0);
        heap.push(Item(0.0, start));
        while let Some(Item(d, n)) = heap.pop() {
            if n == goal {
                break;
            }
            if dist.get(&n).is_some_and(|&x| d > x) {
                continue;
            }
            let l = &self.lines[n.0];
            let mut next: Vec<((usize, usize), f64)> = Vec::new();
            if n.1 > 0 {
                next.push(((n.0, n.1 - 1), l.pts[n.1].distance(l.pts[n.1 - 1])));
            }
            if n.1 + 1 < l.pts.len() {
                next.push(((n.0, n.1 + 1), l.pts[n.1].distance(l.pts[n.1 + 1])));
            }
            for &o in links.get(&n).into_iter().flatten() {
                let q = self.lines[o.0].pts[o.1];
                next.push((o, l.pts[n.1].distance(q)));
            }
            for (m, c) in next {
                let nd = d + c;
                if dist.get(&m).is_none_or(|&x| nd < x) {
                    dist.insert(m, nd);
                    prev.insert(m, n);
                    heap.push(Item(nd, m));
                }
            }
        }
        if !dist.contains_key(&goal) {
            return None;
        }
        let mut nodes = vec![goal];
        let mut n = goal;
        while n != start {
            n = prev[&n];
            nodes.push(n);
        }
        nodes.reverse();
        let mut out: Vec<DVec3> = Vec::new();
        for (li, i) in nodes {
            let l = &self.lines[li];
            let p = DVec3::new(l.pts[i].x, l.floors[i], l.pts[i].y);
            if out.last().is_none_or(|q| q.distance(p) > 0.3) {
                out.push(p);
            }
        }
        Some(out)
    }
}

/// Is a circle at `p` free to stand a pavilion on (no road, pavement,
/// building, prop, landmark or water)?
pub(super) fn spot_free(w: &TownWorld, p: DVec2, r: f64) -> bool {
    super::super::underground::spot_free(w, p, r)
}

/// The tube stretches, stations, gates and lifts of a tile.
pub(super) fn plan_tile(plan: &mut Plan) {
    let w = plan.w;
    let net = w.tube();
    let k = plan.key();
    for &(l, i) in net.stretches.get(&k).into_iter().flatten() {
        let (l, i) = (l as usize, i as usize);
        if let Some(v) = net.stretch_void(l, i) {
            // A lamp every few stretches, in the section's colour.
            let liv = net.livery(w.seed, l, i);
            if (i as u32).is_multiple_of(liv.lamp_every) {
                let line = &net.lines[l];
                let m = (line.pts[i] + line.pts[i + 1]) * 0.5;
                let f = (line.floors[i] + line.floors[i + 1]) * 0.5;
                plan.t.lamps.push(DeepLamp {
                    pos: DVec3::new(m.x, f + TUBE_HEIGHT - 0.4, m.y),
                    color: liv.lamp,
                    strength: 4.0,
                    range: 11.0,
                });
            }
            plan.t
                .tube_parts
                .push((plan.t.voids.len() as u32, l as u32, i as u32));
            plan.t.voids.push(v);
        }
    }
    for &si in net.stations_in(k) {
        station(plan, si);
    }
}

fn station(plan: &mut Plan, si: usize) {
    let w = plan.w;
    let net = w.tube();
    let st = &net.stations[si];
    let hall = st.hall();
    let side = st.dir.perp() * st.side;
    let hall_i = plan.t.voids.len();
    plan.t.voids.push(hall.clone());
    let line = &net.lines[st.line];
    // The gates: short passages from the walkway into the hall.
    let mut gates = Vec::new();
    for gx in GATES {
        let g = st.pos + st.dir * gx;
        // The line's floor there.
        let (fi, _) = line
            .pts
            .iter()
            .enumerate()
            .map(|(j, p)| (j, p.distance(g)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .expect("a line has points");
        let tf = line.floors[fi];
        let into = g + side * (HALL_OFF - HALL_HALF.y + 0.5);
        if let Some(v) = run(VoidKind::Passage, g, into, 1.4, tf, st.floor, 3.1) {
            plan.t.voids.push(v);
        }
        gates.push((g, side));
        plan.t.fixtures.push(Fixture {
            kind: FixtureKind::DestinationBoard,
            at: {
                let b = g + side * (HALL_OFF - HALL_HALF.y + 0.06);
                DVec3::new(b.x, st.floor + 3.4, b.y)
            },
            dir: side,
            size: DVec3::new(1.6, 0.6, 0.05),
            look: si as u32,
        });
    }
    // The lift up to the street, beside the road on the hall's side.
    let off = line.verge + SHAFT_HALF + 1.6;
    let mut entrance = None;
    // Nearest first: along the hall, then further out along the road and
    // back from the kerb.
    let mut tries: Vec<(f64, f64)> = Vec::new();
    for k in 0..=10 {
        for e in [0.0, 3.0, 6.0, 9.0] {
            for s in [1.0, -1.0] {
                if k > 0 || s > 0.0 {
                    tries.push((s * 6.0 * k as f64, e));
                }
            }
        }
    }
    tries.sort_by(|a, b| (a.0.abs() + a.1 * 2.0).total_cmp(&(b.0.abs() + b.1 * 2.0)));
    let others = net.stretch_voids_near(st.pos, 100.0);
    for (x, e) in tries {
        let c = st.pos + st.dir * x + side * (off + e);
        if !spot_free(w, c, SHAFT_HALF + 1.2) {
            continue;
        }
        let l = hall.local(c);
        let inside = l.x.abs() <= HALL_HALF.x - SHAFT_HALF - 0.6
            && l.y.abs() <= HALL_HALF.y - SHAFT_HALF - 0.6;
        let passage = if inside {
            None
        } else {
            let q = hall.world(DVec2::new(
                l.x.clamp(-HALL_HALF.x + 1.5, HALL_HALF.x - 1.5),
                l.y.clamp(-HALL_HALF.y + 1.5, HALL_HALF.y - 1.5),
            ));
            let v = run(VoidKind::Passage, q, c, 1.5, st.floor, st.floor, 3.2);
            match v {
                Some(v)
                    if plan.roofed(&v, 1.5)
                        && plan.clear_of_shallow(&v, 1.0)
                        && others.iter().all(|(_, _, o)| !overlaps(&v, o, 1.0)) =>
                {
                    Some(v)
                }
                _ => continue,
            }
        };
        let shaft = Void {
            kind: VoidKind::Shaft,
            origin: c - DVec2::splat(SHAFT_HALF),
            along: DVec2::X,
            min: DVec2::ZERO,
            max: DVec2::splat(SHAFT_HALF * 2.0),
            floor: [st.floor, st.floor],
            height: 0.0,
            open: true,
        };
        if !plan.clear_of_shallow(&shaft, 0.8) {
            continue;
        }
        plan.t.voids.push(shaft);
        plan.t.voids.extend(passage);
        entrance = Some(c);
        break;
    }
    if let Some(c) = entrance {
        plan.t.entrances.push(DeepEntrance {
            kind: EntranceKind::Tube,
            name: st.name.clone(),
            pos: c,
            door: -side,
            top: w.ground(c),
            bottom: st.floor,
        });
    }
    if st.deep_lift {
        plan.lift_to_deeps(st.deep_lift_pos(), st.floor);
    }
    // Benches along the back wall, planters between, lamps overhead.
    let back = HALL_HALF.y - 0.9;
    for k in 0..6 {
        let x = -20.0 + k as f64 * 8.0;
        let p = hall.world(DVec2::new(x, st.side * back));
        if entrance.is_some_and(|c| c.distance(p) < SHAFT_HALF + 2.5)
            || (st.deep_lift && st.deep_lift_pos().distance(p) < SHAFT_HALF + 2.5)
        {
            continue;
        }
        let kind = if k % 2 == 0 {
            FixtureKind::Bench
        } else {
            FixtureKind::Planter
        };
        let size = if k % 2 == 0 {
            DVec3::new(1.4, 0.9, 0.35)
        } else {
            DVec3::new(0.55, 1.0, 0.55)
        };
        plan.t.fixtures.push(Fixture {
            kind,
            at: DVec3::new(p.x, st.floor, p.y),
            dir: -side,
            size,
            look: k,
        });
    }
    for k in 0..4 {
        let x = -19.5 + k as f64 * 13.0;
        let p = hall.world(DVec2::new(x, 0.0));
        plan.t.lamps.push(DeepLamp {
            pos: DVec3::new(p.x, st.floor + HALL_HEIGHT - 0.8, p.y),
            color: [1.0, 0.88, 0.7],
            strength: 7.0,
            range: 14.0,
        });
    }
    plan.t.stations.push(StationHall {
        station: si,
        hall: hall_i,
        gates,
    });
}
