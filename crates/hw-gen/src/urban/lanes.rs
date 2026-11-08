//! Lanes for traffic: a lane each way along every road, joined at the
//! junctions into closed loops.
//!
//! The roads are polylines that meet anywhere: a side street ends on the
//! side of a main road, main roads end at a shared point. First every road
//! is cut where another one ends on it, the cut points and road ends that
//! lie together become junctions, and the pieces between junctions become
//! edges with one lane each way (on the right).
//!
//! At each junction every lane coming in is paired with one lane going out
//! (mostly straight on, some turning; a dead end turns round). Because each
//! lane then has exactly one lane after it and one before it, following
//! them splits all the lanes into closed loops that never share a lane. A
//! loop is a single smooth line: cars spaced evenly on it and driving at one
//! speed never run into each other, and where a car is is a pure function
//! of time (see `hw_sim::traffic`).

use super::world::{TownWorld, WorldRoad};
use crate::geom::{locate, Polyline};
use glam::DVec2;
use hw_core::{det, Seed};
use std::collections::BTreeMap;

/// Road ends closer than this to another road (beyond its half width)
/// meet it.
const JOIN: f64 = 4.0;
/// Junction points closer than this are one junction.
const MERGE: f64 = 6.0;
/// A stop cuts its road this far before and after it (m): more than half
/// of [`MERGE`], so the two sides stay apart.
const STOP_CUT: f64 = 4.0;
/// Edges shorter than this between the same junction are dropped.
const MIN_LOOP_EDGE: f64 = 15.0;
/// Grid cell for finding roads near a point.
const CELL: f64 = 64.0;

/// One closed loop of lanes: a smooth line through the town.
#[derive(Clone, Debug)]
pub struct LaneLoop {
    pub pts: Vec<DVec2>,
    /// Distance along the loop at each point (the last is the loop's
    /// length, back at the first point).
    pub at: Vec<f64>,
    /// Main road share of the loop (0 to 1).
    pub main: f64,
}

impl LaneLoop {
    pub fn len(&self) -> f64 {
        *self.at.last().unwrap_or(&0.0)
    }

    pub fn is_empty(&self) -> bool {
        self.pts.len() < 2
    }

    /// Point and heading `s` metres along the loop (any `s`: it wraps).
    pub fn sample(&self, s: f64) -> (DVec2, DVec2) {
        let len = self.len();
        if len <= 0.0 || self.pts.len() < 2 {
            return (self.pts.first().copied().unwrap_or_default(), DVec2::Y);
        }
        let (k, t) = locate(&self.at, s.rem_euclid(len));
        let (a, b) = (self.pts[k], self.pts[k + 1]);
        (a.lerp(b, t), (b - a).normalize_or(DVec2::Y))
    }

    /// Stretches of the loop inside each grid cell of `cell` metres:
    /// (cell, from, to) in distance along the loop.
    pub fn spans(&self, cell: f64) -> Vec<((i64, i64), f64, f64)> {
        let mut out: Vec<((i64, i64), f64, f64)> = Vec::new();
        let key = |p: DVec2| ((p.x / cell).floor() as i64, (p.y / cell).floor() as i64);
        for k in 0..self.pts.len().saturating_sub(1) {
            let (a, b) = (self.pts[k], self.pts[k + 1]);
            let l = self.at[k + 1] - self.at[k];
            let n = (l / (cell * 0.25)).ceil().max(1.0) as usize;
            for j in 0..n {
                let s0 = self.at[k] + l * j as f64 / n as f64;
                let s1 = self.at[k] + l * (j + 1) as f64 / n as f64;
                let c = key(a.lerp(b, (j as f64 + 0.5) / n as f64));
                match out.last_mut() {
                    Some(last) if last.0 == c && (last.2 - s0).abs() < 1e-6 => last.2 = s1,
                    _ => out.push((c, s0, s1)),
                }
            }
        }
        out
    }
}

/// A piece of road between two junctions.
struct Edge {
    road: usize,
    s0: f64,
    s1: f64,
    a: usize,
    b: usize,
}

/// Plans the lane loops on the roads `roads` (indices into `world.roads`),
/// a lane each way, `offset(road)` metres right of the centre line. No
/// lane passes a point of `stops` (a barricade across the road): cars
/// turn round before it, on either side.
pub fn plan_lanes(
    world: &TownWorld,
    roads: &[usize],
    offset: impl Fn(&WorldRoad) -> f64,
    stops: &[DVec2],
    seed: Seed,
) -> Vec<LaneLoop> {
    let lines: Vec<Polyline> = roads
        .iter()
        .map(|&r| Polyline::new(&world.roads[r].pts))
        .collect();
    let half = |i: usize| world.roads[roads[i]].half;
    // Roads by grid cell.
    let cell = |p: DVec2| ((p.x / CELL).floor() as i64, (p.y / CELL).floor() as i64);
    let mut grid: BTreeMap<(i64, i64), Vec<usize>> = BTreeMap::new();
    for (i, l) in lines.iter().enumerate() {
        let mut cells = Vec::new();
        for k in 0..l.pts.len() - 1 {
            let n = (l.pts[k].distance(l.pts[k + 1]) / (CELL * 0.5))
                .ceil()
                .max(1.0) as usize;
            for j in 0..=n {
                cells.push(cell(l.pts[k].lerp(l.pts[k + 1], j as f64 / n as f64)));
            }
        }
        cells.sort();
        cells.dedup();
        for c in cells {
            grid.entry(c).or_default().push(i);
        }
    }
    // Junction points: every road end, and every point where another road
    // ends on a road. Points that meet are joined (union-find).
    let mut joints: Vec<(usize, f64, DVec2)> = Vec::new();
    let mut parent: Vec<usize> = Vec::new();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    let mut unions: Vec<(usize, usize)> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        for s in [0.0, l.len()] {
            let p = l.point(s);
            let me = joints.len();
            joints.push((i, s, p));
            parent.push(me);
            let c = cell(p);
            let mut near = Vec::new();
            for dx in -1..=1 {
                for dy in -1..=1 {
                    if let Some(list) = grid.get(&(c.0 + dx, c.1 + dy)) {
                        near.extend(list.iter().copied());
                    }
                }
            }
            near.sort();
            near.dedup();
            for j in near {
                if j == i {
                    continue;
                }
                let (sj, d) = lines[j].nearest(p);
                if d <= half(j) + JOIN {
                    let other = joints.len();
                    joints.push((j, sj, lines[j].point(sj)));
                    parent.push(other);
                    unions.push((me, other));
                }
            }
        }
    }
    // Each stop cuts its road twice, a little before and after it; the
    // piece between is left out below, so both sides end there.
    let mut cut_joints: Vec<(usize, usize, usize)> = Vec::new();
    for &p in stops {
        let c = cell(p);
        let mut best: Option<(usize, f64, f64)> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for &i in grid.get(&(c.0 + dx, c.1 + dy)).into_iter().flatten() {
                    let (s, d) = lines[i].nearest(p);
                    if d <= half(i) && best.is_none_or(|b| d < b.2) {
                        best = Some((i, s, d));
                    }
                }
            }
        }
        let Some((i, s, _)) = best else { continue };
        let mut pair = [0; 2];
        for (k, at) in [(s - STOP_CUT).max(0.0), (s + STOP_CUT).min(lines[i].len())]
            .into_iter()
            .enumerate()
        {
            pair[k] = joints.len();
            joints.push((i, at, lines[i].point(at)));
            parent.push(pair[k]);
        }
        cut_joints.push((i, pair[0], pair[1]));
    }
    // Points close together are one junction.
    let mut jgrid: BTreeMap<(i64, i64), Vec<usize>> = BTreeMap::new();
    let jcell = |p: DVec2| ((p.x / MERGE).floor() as i64, (p.y / MERGE).floor() as i64);
    for (k, j) in joints.iter().enumerate() {
        jgrid.entry(jcell(j.2)).or_default().push(k);
    }
    for (k, j) in joints.iter().enumerate() {
        let c = jcell(j.2);
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(list) = jgrid.get(&(c.0 + dx, c.1 + dy)) {
                    for &o in list {
                        if o > k && joints[o].2.distance(j.2) < MERGE {
                            unions.push((k, o));
                        }
                    }
                }
            }
        }
    }
    for (a, b) in unions {
        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
        if ra != rb {
            parent[ra.max(rb)] = ra.min(rb);
        }
    }
    // Junctions: one per root, at the mean of its points.
    let mut node_of_root: BTreeMap<usize, usize> = BTreeMap::new();
    let mut node_sum: Vec<(DVec2, f64)> = Vec::new();
    let mut joint_node = vec![0usize; joints.len()];
    for k in 0..joints.len() {
        let r = find(&mut parent, k);
        let n = *node_of_root.entry(r).or_insert_with(|| {
            node_sum.push((DVec2::ZERO, 0.0));
            node_sum.len() - 1
        });
        joint_node[k] = n;
        node_sum[n].0 += joints[k].2;
        node_sum[n].1 += 1.0;
    }
    let nodes: Vec<DVec2> = node_sum.iter().map(|(s, n)| *s / *n).collect();
    let cut: Vec<(usize, usize, usize)> = cut_joints
        .iter()
        .map(|&(i, a, b)| (i, joint_node[a], joint_node[b]))
        .collect();
    // Edges: each road cut at its junction points.
    let mut per_road: Vec<Vec<(f64, usize)>> = vec![Vec::new(); lines.len()];
    for (k, j) in joints.iter().enumerate() {
        per_road[j.0].push((j.1, joint_node[k]));
    }
    let mut edges: Vec<Edge> = Vec::new();
    for (i, cuts) in per_road.iter_mut().enumerate() {
        cuts.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        // Neighbouring cuts at one junction are one cut.
        let mut merged: Vec<(f64, usize)> = Vec::new();
        for &(s, n) in cuts.iter() {
            match merged.last_mut() {
                Some(last) if last.1 == n => last.0 = (last.0 + s) * 0.5,
                _ => merged.push((s, n)),
            }
        }
        // The road's own ends stay where they are.
        if let Some(first) = merged.first_mut() {
            if first.0 < MERGE {
                first.0 = 0.0;
            }
        }
        let len = lines[i].len();
        if let Some(last) = merged.last_mut() {
            if (last.0 - len).abs() < MERGE {
                last.0 = len;
            }
        }
        for w in merged.windows(2) {
            let (s0, a) = w[0];
            let (s1, b) = w[1];
            let l = s1 - s0;
            if l < 1.0 || (a == b && l < MIN_LOOP_EDGE) {
                continue;
            }
            // The stretch across a barricade.
            if cut
                .iter()
                .any(|&(r, x, y)| r == i && ((a, b) == (x, y) || (a, b) == (y, x)))
            {
                continue;
            }
            edges.push(Edge {
                road: i,
                s0,
                s1,
                a,
                b,
            });
        }
    }
    // The edge ends at each junction, and the way each one leaves it.
    let mut ends: Vec<Vec<(usize, bool, DVec2)>> = vec![Vec::new(); nodes.len()];
    for (e, ed) in edges.iter().enumerate() {
        let l = &lines[ed.road];
        let reach = ((ed.s1 - ed.s0) * 0.5).min(10.0);
        let out_a = (l.point(ed.s0 + reach) - nodes[ed.a]).normalize_or(DVec2::X);
        let out_b = (l.point(ed.s1 - reach) - nodes[ed.b]).normalize_or(DVec2::X);
        ends[ed.a].push((e, true, out_a));
        ends[ed.b].push((e, false, out_b));
    }
    // Junction size: the widest road meeting there.
    let radius: Vec<f64> = (0..nodes.len())
        .map(|n| {
            ends[n]
                .iter()
                .map(|(e, _, _)| world.roads[roads[edges[*e].road]].half)
                .fold(0.0, f64::max)
                + 1.5
        })
        .collect();
    // Pair lanes in with lanes out at each junction: a turn of the ends
    // sorted round the junction, mostly the straightest.
    let mut pair: Vec<Vec<usize>> = Vec::with_capacity(nodes.len());
    for (n, list) in ends.iter_mut().enumerate() {
        list.sort_by(|a, b| {
            det::atan2(a.2.y, a.2.x)
                .total_cmp(&det::atan2(b.2.y, b.2.x))
                .then(a.0.cmp(&b.0))
                .then(a.1.cmp(&b.1))
        });
        let k = list.len();
        let p: Vec<usize> = match k {
            0 => Vec::new(),
            1 => vec![0],
            2 => vec![1, 0],
            _ => {
                let straight = |r: usize| -> f64 {
                    (0..k)
                        .map(|i| -list[i].2.dot(list[(i + r) % k].2))
                        .sum::<f64>()
                };
                let mut best = 1;
                for r in 2..k {
                    if straight(r) > straight(best) {
                        best = r;
                    }
                }
                let mut rng = seed.child("junction").child(n as u64).rng();
                let r = if rng.chance(0.65) {
                    best
                } else {
                    rng.range_int(1, k as i64) as usize
                };
                (0..k).map(|i| (i + r) % k).collect()
            }
        };
        pair.push(p);
    }
    // Lanes: 2e goes from a to b, 2e + 1 from b to a. Each lane's geometry,
    // trimmed back from the junctions and moved to its side of the road.
    let lane_pts = |lane: usize| -> Vec<DVec2> {
        let ed = &edges[lane / 2];
        let l = &lines[ed.road];
        let len = ed.s1 - ed.s0;
        let ta = radius[ed.a].min(len * 0.4);
        let tb = radius[ed.b].min(len * 0.4);
        let pts = if lane.is_multiple_of(2) {
            l.slice(ed.s0 + ta, ed.s1 - tb)
        } else {
            l.slice(ed.s1 - tb, ed.s0 + ta)
        };
        offset_line(&pts, offset(&world.roads[roads[ed.road]]))
    };
    // Where each end sits in its junction's list.
    let mut slot: BTreeMap<(usize, bool), (usize, usize)> = BTreeMap::new();
    for (n, list) in ends.iter().enumerate() {
        for (i, (e, start, _)) in list.iter().enumerate() {
            slot.insert((*e, *start), (n, i));
        }
    }
    let next = |lane: usize| -> usize {
        let e = lane / 2;
        // A lane a -> b arrives at b (the edge's end), b -> a at a.
        let arrive = (e, lane % 2 == 1);
        let (n, i) = slot[&arrive];
        let (e2, start2, _) = ends[n][pair[n][i]];
        // Leaving from the edge's start goes a -> b.
        if start2 {
            e2 * 2
        } else {
            e2 * 2 + 1
        }
    };
    let mut seen = vec![false; edges.len() * 2];
    let mut loops = Vec::new();
    for first in 0..edges.len() * 2 {
        if seen[first] {
            continue;
        }
        let mut order = Vec::new();
        let mut lane = first;
        while !seen[lane] {
            seen[lane] = true;
            order.push(lane);
            lane = next(lane);
        }
        let mut pts: Vec<DVec2> = Vec::new();
        let mut main_len = 0.0;
        let mut all_len = 0.0;
        let geo: Vec<Vec<DVec2>> = order.iter().map(|&l| lane_pts(l)).collect();
        for (k, g) in geo.iter().enumerate() {
            let l: f64 = g.windows(2).map(|w| w[0].distance(w[1])).sum();
            all_len += l;
            if world.roads[roads[edges[order[k] / 2].road]].main {
                main_len += l;
            }
            pts.extend(g.iter().copied());
            // Round the corner to the next lane.
            let h = &geo[(k + 1) % geo.len()];
            if g.len() >= 2 && h.len() >= 2 {
                let (p0, d0) = (
                    g[g.len() - 1],
                    (g[g.len() - 1] - g[g.len() - 2]).normalize(),
                );
                let (p1, d1) = (h[0], (h[1] - h[0]).normalize());
                let k = (p0.distance(p1) * 0.45).max(3.0);
                let (c0, c1) = (p0 + d0 * k, p1 - d1 * k);
                let steps = 8;
                for j in 1..steps {
                    let t = j as f64 / steps as f64;
                    let u = 1.0 - t;
                    pts.push(
                        p0 * (u * u * u)
                            + c0 * (3.0 * u * u * t)
                            + c1 * (3.0 * u * t * t)
                            + p1 * (t * t * t),
                    );
                }
            }
        }
        if pts.len() < 3 {
            continue;
        }
        pts.push(pts[0]);
        pts.dedup_by(|a, b| a.distance(*b) < 0.05);
        let mut at = vec![0.0];
        for w in pts.windows(2) {
            at.push(at.last().unwrap() + w[0].distance(w[1]));
        }
        if *at.last().unwrap() < 20.0 {
            continue;
        }
        loops.push(LaneLoop {
            pts,
            at,
            main: if all_len > 0.0 {
                main_len / all_len
            } else {
                0.0
            },
        });
    }
    loops
}

/// The lanes street cars drive: on every road with room for a lane each
/// way, clear of the parked cars, turning round before the barricades.
/// The same for the traffic and for the street furniture, which keeps out
/// of their way.
pub fn street_lanes(world: &TownWorld) -> Vec<LaneLoop> {
    let seed = world.seed.child("traffic").child("ground");
    let streets: Vec<usize> = (0..world.roads.len())
        .filter(|&i| world.roads[i].has_traffic())
        .collect();
    let stops: Vec<DVec2> = super::edge::barricades(world)
        .iter()
        .map(|b| b.at)
        .collect();
    plan_lanes(world, &streets, WorldRoad::lane_offset, &stops, seed)
}

/// A polyline moved `d` metres to its right (x, y plan; right is `perp`).
fn offset_line(pts: &[DVec2], d: f64) -> Vec<DVec2> {
    let n = pts.len();
    if n < 2 {
        return pts.to_vec();
    }
    (0..n)
        .map(|i| {
            let a = if i > 0 {
                (pts[i] - pts[i - 1]).normalize_or_zero()
            } else {
                DVec2::ZERO
            };
            let b = if i + 1 < n {
                (pts[i + 1] - pts[i]).normalize_or_zero()
            } else {
                DVec2::ZERO
            };
            let t = (a + b).normalize_or(b + a);
            let nrm = t.perp();
            // Mitre, kept from growing at sharp corners.
            let scale = if i > 0 && i + 1 < n {
                1.0 / nrm.dot(b.perp()).max(0.5)
            } else {
                1.0
            };
            pts[i] + nrm * d * scale
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::urban::TownParams;

    fn town() -> TownWorld {
        TownWorld::generate(Seed(1), TownParams::sample(Seed(1), 20_000.0))
    }

    /// A car driving the street lanes never passes through a building, a
    /// parked car, a lamp, a tree, a kiosk or a barricade: what cars drive
    /// through must be open road. (Before 0.065 one sample in eight hit a
    /// house or a parked car.)
    #[test]
    fn street_cars_keep_clear_of_buildings_and_props() {
        use crate::urban::world::CAR_HALF;
        use std::collections::BTreeMap;
        for s in [1u64, 3] {
            let w = TownWorld::generate(Seed(s), TownParams::sample(Seed(s), 40_000.0));
            let key = |p: DVec2| ((p.x / 16.0).floor() as i64, (p.y / 16.0).floor() as i64);
            let mut solids: BTreeMap<(i64, i64), Vec<crate::urban::footprint::Footprint>> =
                BTreeMap::new();
            for p in &w.props {
                for f in p.footprints() {
                    solids.entry(key(f.at())).or_default().push(f);
                }
            }
            for b in &w.buildings {
                let st = &b.site;
                let f = crate::urban::footprint::Footprint::Box {
                    at: st.origin + st.along * st.size.x * 0.5 + st.inward * st.size.y * 0.5,
                    dir: st.inward,
                    half: st.size * 0.5,
                    y0: 0.0,
                    y1: 1.0,
                };
                // Filed in every cell it covers.
                let r = (f.reach() / 16.0).ceil() as i64;
                let k = key(f.at());
                for dx in -r..=r {
                    for dy in -r..=r {
                        solids.entry((k.0 + dx, k.1 + dy)).or_default().push(f);
                    }
                }
            }
            let (mut samples, mut hits) = (0usize, Vec::new());
            for l in street_lanes(&w) {
                let mut at = 0.0;
                while at < l.len() {
                    let (p, d) = l.sample(at);
                    samples += 1;
                    // The car as three discs along its length.
                    let hit = [p - d * 1.3, p, p + d * 1.3].iter().any(|&c| {
                        let k = key(c);
                        (-1..=1).any(|dx| {
                            (-1..=1).any(|dy| {
                                solids
                                    .get(&(k.0 + dx, k.1 + dy))
                                    .into_iter()
                                    .flatten()
                                    .any(|f| f.distance(c) < CAR_HALF - 0.2)
                            })
                        })
                    });
                    if hit {
                        hits.push(p);
                    }
                    at += 2.0;
                }
            }
            assert!(
                hits.len() * 2000 <= samples,
                "seed {s}: {} of {samples} lane samples run into something, e.g. {:?}",
                hits.len(),
                &hits[..hits.len().min(5)]
            );
        }
    }

    #[test]
    fn every_lane_is_on_a_loop_and_loops_are_smooth() {
        let w = town();
        let all: Vec<usize> = (0..w.roads.len()).collect();
        let loops = plan_lanes(&w, &all, |r| r.half * 0.5, &[], Seed(5));
        assert!(loops.len() > 10, "{} loops", loops.len());
        let total: f64 = loops.iter().map(|l| l.len()).sum();
        let roads: f64 = w
            .roads
            .iter()
            .map(|r| r.pts.windows(2).map(|p| p[0].distance(p[1])).sum::<f64>())
            .sum();
        // About two lanes per road (less what the junctions take).
        assert!(
            total > roads * 1.4 && total < roads * 2.6,
            "lanes {total:.0} m for roads {roads:.0} m"
        );
        for l in &loops {
            // Closed, and no jumps.
            assert!(l.pts[0].distance(*l.pts.last().unwrap()) < 0.1);
            for p in l.pts.windows(2) {
                // No jumps from one lane to a far one (long straight road
                // pieces are fine).
                let d = p[0].distance(p[1]);
                let (m, dir) = ((p[0] + p[1]) * 0.5, (p[1] - p[0]) / d.max(1e-9));
                let on_road = w.roads.iter().any(|r| {
                    r.pts.windows(2).any(|q| {
                        let u = (q[1] - q[0]).normalize_or_zero();
                        let t = (m - q[0]).dot(u).clamp(0.0, q[0].distance(q[1]));
                        u.dot(dir).abs() > 0.95 && m.distance(q[0] + u * t) < r.half + 3.0
                    })
                });
                assert!(d < 60.0 || on_road, "jump of {d:.0} m at {:?}", p[0]);
            }
            let (a, _) = l.sample(l.len() - 0.01);
            let (b, _) = l.sample(0.01);
            assert!(a.distance(b) < 0.2);
        }
    }

    #[test]
    fn same_seed_same_lanes() {
        let (a, b) = (town(), town());
        let main_a: Vec<usize> = (0..a.roads.len()).filter(|&i| a.roads[i].main).collect();
        let x = plan_lanes(&a, &main_a, |_| 2.0, &[], Seed(2));
        let y = plan_lanes(&b, &main_a, |_| 2.0, &[], Seed(2));
        assert_eq!(x.len(), y.len());
        for (p, q) in x.iter().zip(&y) {
            assert_eq!(p.pts, q.pts);
        }
    }
}
