//! How the settlement grew: its centres, the old country roads that meet
//! at it, and how far it spread, when.
//!
//! Growth is a race outwards from the centres across the land. Flat land is
//! quick to build on, slopes slow it down, rivers have to be bridged, the
//! sea and lakes stop it, and the old roads speed it up (more so the more
//! the town sprawls). The town is everything reached before the time at
//! which its land holds its population. That gives every town its own
//! outline: a half-moon on a bay, a ribbon in a valley, a star along its
//! roads.
//!
//! The time at which a cell was reached is its age: the old town is what
//! was reached first, then the planned extensions, then the suburbs.
//! Villages on the old roads are reached from their own centres and keep
//! their own little cores.

use super::land::{Cover, Grid, Land};
use super::params::{Founding, TownParams};
use glam::DVec2;
use hw_core::{det, Noise, Seed};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// A centre the town grew from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CentreKind {
    /// The founding point and its old town.
    Core,
    /// A village on an old road, swallowed by the town.
    Village,
    /// A business district (big cities).
    Business,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Centre {
    pub pos: DVec2,
    pub kind: CentreKind,
    /// How much later than the core it started growing, in metres of
    /// travel (villages start late, so they stay smaller).
    pub head: f64,
    /// Growth (m) a village has of its own, built even when the town's
    /// edge has not reached it.
    pub reach: f64,
}

/// The age of a piece of town.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Era {
    /// The old town (or the old core of a swallowed village).
    Old,
    /// The planned and unplanned extensions of the 19th century.
    Extension,
    /// Everything since: suburbs, estates, the edge.
    Modern,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Growth {
    pub centres: Vec<Centre>,
    /// Where the old roads leave the map, towards neighbouring places.
    pub gates: Vec<DVec2>,
    /// The old country roads from the core out to the gates, split where
    /// they fork, smoothed. Pieces meet exactly at their forks.
    pub roads: Vec<Vec<DVec2>>,
    /// Growth time per cell (m of travel); infinite where nothing grows.
    pub time: Grid<f32>,
    /// The centre each cell grew from.
    pub owner: Grid<u16>,
    /// Growth time at the edge of the built-up area.
    pub limit: f32,
    /// Growth times ending the old town and the extensions.
    pub eras: [f32; 2],
    /// Growth times (from the core) of former town walls, oldest first.
    pub walls: Vec<f32>,
}

impl Growth {
    pub fn generate(seed: Seed, p: &TownParams, land: &Land) -> Growth {
        let seed = seed.child("growth");
        let mut rng = seed.rng();
        let g = &land.height;
        let s = p.scale();
        let n = g.w * g.h;
        let slope = slopes(land);

        // --- Old roads: least-cost paths from the core to the gates.
        let lp = det::log10(p.population);
        let gate_count = ((2.0 + 1.1 * lp * rng.range(0.7, 1.2)).round() as usize).clamp(2, 10);
        // Fields, woods and estates the roads wind around.
        let wander = Noise::new(seed.child("wander"));
        let wl = (s * 0.5).max(150.0);
        let road_cost: Vec<f32> = (0..n)
            .map(|k| match land.cover.data[k] {
                Cover::Sea | Cover::Lake => f32::INFINITY,
                // Fords and bridges are few: roads gather to cross.
                Cover::River(_) => 25.0,
                Cover::Land => {
                    let q = g.center(k % g.w, k / g.w);
                    let w = (1.0 + 0.7 * wander.fbm(q.x / wl, q.y / wl, 3)).max(0.3) as f32;
                    (1.0 + 120.0 * slope[k] * slope[k]) * w
                }
            })
            .collect();
        let core_cell = g.cell_of(DVec2::ZERO).expect("core on the map");
        let (_, via) = dijkstra(g, &road_cost, &[(core_cell, 0.0)]);
        let mut gates = Vec::new();
        let base_angle = rng.range(0.0, std::f64::consts::TAU);
        let angles: Vec<f64> = if p.links.is_empty() {
            (0..gate_count)
                .map(|k| {
                    base_angle
                        + k as f64 / gate_count as f64 * std::f64::consts::TAU
                        + rng.range(-0.35, 0.35) * std::f64::consts::TAU / gate_count as f64
                })
                .collect()
        } else {
            p.links.clone()
        };
        for a in angles {
            if let Some(cell) = edge_cell(land, det::dir2(a), &via) {
                let q = g.center(cell.0, cell.1);
                if gates.iter().all(|o: &DVec2| o.distance(q) > g.half() * 0.4) {
                    gates.push(q);
                }
            }
        }
        let routes: Vec<Vec<DVec2>> = gates
            .iter()
            .map(|q| {
                let path = trace(g, &via, g.cell_of(*q).expect("gate on the map"));
                simplify(&path, g.cell * 0.8)
            })
            .collect();
        let roads = branches(g, &via, &gates, core_cell);

        // --- Centres: the core, villages on the old roads, business districts.
        let mut centres = vec![Centre {
            pos: DVec2::ZERO,
            kind: CentreKind::Core,
            head: 0.0,
            reach: 0.0,
        }];
        let mut tries = 0;
        while centres
            .iter()
            .filter(|c| c.kind == CentreKind::Village)
            .count()
            < p.villages
            && tries < p.villages * 30 + 30
        {
            tries += 1;
            // Most villages sit on an old road, some off them.
            let at = if !routes.is_empty() && rng.chance(0.75) {
                let r = &routes[rng.index(routes.len())];
                point_along(r, s * rng.range(0.7, 2.6))
            } else {
                Some(det::dir2(rng.range(0.0, std::f64::consts::TAU)) * s * rng.range(0.8, 2.4))
            };
            let Some(at) = at else { continue };
            if land.cover_at(at).is_water() || g.cell_of(at).is_none() {
                continue;
            }
            let gap = s * 0.45;
            if centres.iter().any(|c| c.pos.distance(at) < gap) {
                continue;
            }
            centres.push(Centre {
                pos: at,
                kind: CentreKind::Village,
                head: at.length() * rng.range(0.2, 0.5),
                reach: s * rng.range(0.06, 0.16) + 40.0,
            });
        }
        for _ in 0..p.business_districts {
            for _ in 0..40 {
                let at =
                    det::dir2(rng.range(0.0, std::f64::consts::TAU)) * s * rng.range(0.25, 0.7);
                if land.cover_at(at).is_water()
                    || centres.iter().any(|c| c.pos.distance(at) < s * 0.25)
                {
                    continue;
                }
                centres.push(Centre {
                    pos: at,
                    kind: CentreKind::Business,
                    head: at.length() * 0.35,
                    reach: 0.0,
                });
                break;
            }
        }

        // --- Growth from all centres at once.
        let mut on_road = vec![false; n];
        for r in &roads {
            stamp_line(g, r, g.cell * 0.8, &mut on_road);
        }
        let noise = Noise::new(seed.child("edge"));
        let fray = (s * 0.35).max(60.0);
        let road_speed = (1.0 - 0.75 * p.sprawl) as f32;
        let core_r = s * 0.12 + 50.0;
        let grow_cost: Vec<f32> = (0..n)
            .map(|k| match land.cover.data[k] {
                Cover::Sea | Cover::Lake => f32::INFINITY,
                Cover::River(_) => 7.0,
                Cover::Land => {
                    let q = g.center(k % g.w, k / g.w);
                    let rough = (1.0
                        + 0.45 * noise.fbm(q.x / fray, q.y / fray, 4)
                        + 0.3 * noise.fbm(q.x / (fray * 4.0) + 50.0, q.y / (fray * 4.0), 2))
                    .max(0.25);
                    let sl = slope[k] as f64;
                    let steep = 1.0 + 60.0 * sl * sl + if sl > 0.3 { 8.0 } else { 0.0 };
                    // Old towns are compact; ribbons along roads came later.
                    let road = if on_road[k] {
                        let r = super::land::smooth(core_r, core_r * 2.5, q.length()) as f32;
                        1.0 + (road_speed - 1.0) * r
                    } else {
                        1.0
                    };
                    (rough * steep) as f32 * road
                }
            })
            .collect();
        let sources: Vec<((usize, usize), f64)> = centres
            .iter()
            .filter_map(|c| g.cell_of(c.pos).map(|cell| (cell, c.head)))
            .collect();
        let (time, via_g) = dijkstra(g, &grow_cost, &sources);
        // Who each cell belongs to: follow its path back to a source.
        let mut owner = Grid {
            origin: g.origin,
            cell: g.cell,
            w: g.w,
            h: g.h,
            data: vec![u16::MAX; n],
        };
        let mut order: Vec<usize> = (0..n).filter(|k| time[*k].is_finite()).collect();
        order.sort_by(|a, b| time[*a].total_cmp(&time[*b]).then(a.cmp(b)));
        for (ci, (cell, _)) in sources.iter().enumerate() {
            owner.data[cell.1 * g.w + cell.0] = ci as u16;
        }
        for &k in &order {
            if owner.data[k] == u16::MAX {
                if let Some(from) = via_g[k] {
                    owner.data[k] = owner.data[from as usize];
                }
            }
        }
        // Rivers carry growth across but are never built on.
        let mut time = time;
        for (k, c) in land.cover.data.iter().enumerate() {
            if c.is_water() {
                time[k] = f32::INFINITY;
            }
        }
        order.retain(|k| time[*k].is_finite());
        // The edge: when the land reached holds the population.
        let cell_area = g.cell * g.cell;
        let want = (p.built_area() / cell_area).ceil() as usize;
        let limit = order
            .get(want.min(order.len().saturating_sub(1)))
            .map_or(0.0, |k| time[*k]);
        // Eras by how many people each age housed.
        let old_pop = match p.founding {
            Founding::Colony => 4_000.0,
            Founding::Fortress => 25_000.0,
            _ => 15_000.0,
        } * rng.range(0.6, 1.6);
        let ext_pop = old_pop * rng.range(8.0, 30.0);
        // Old towns were packed: their people took far less land than
        // today's (people per km² in the old town and the extensions).
        let old_density = rng.range(12_000.0, 25_000.0);
        let ext_density = rng.range(6_000.0, 12_000.0);
        let cells_for = |pop: f64| {
            let old = pop.min(old_pop) / old_density;
            let ext = (pop - old_pop).max(0.0) / ext_density;
            (((old + ext) * 1e6 / cell_area) as usize).min(want)
        };
        let time_at = |k: usize| {
            order
                .get(k.min(order.len().saturating_sub(1)))
                .map_or(0.0, |k| time[*k])
        };
        let eras = [time_at(cells_for(old_pop)), time_at(cells_for(ext_pop))];
        // Walls followed the edge of the town at the time they were built.
        let mut walls = Vec::new();
        for w in 0..p.walls {
            let pop = old_pop * det::powf(4.0, w as f64) * rng.range(0.5, 0.9);
            if pop < p.population * 0.8 {
                walls.push(time_at(cells_for(pop)));
            }
        }
        Growth {
            centres,
            gates,
            roads,
            time: Grid {
                origin: g.origin,
                cell: g.cell,
                w: g.w,
                h: g.h,
                data: time,
            },
            owner,
            limit,
            eras,
            walls,
        }
    }

    /// Is cell (i, j) built up?
    pub fn built(&self, i: usize, j: usize) -> bool {
        let t = *self.time.at(i, j);
        if t <= self.limit {
            return true;
        }
        // Villages beyond the edge are still there, as small places.
        let o = *self.owner.at(i, j) as usize;
        self.centres
            .get(o)
            .is_some_and(|c| t as f64 - c.head <= c.reach)
    }

    /// The era of a built cell.
    pub fn era(&self, i: usize, j: usize) -> Era {
        let t = *self.time.at(i, j);
        if t <= self.eras[0] {
            Era::Old
        } else if t <= self.eras[1] {
            Era::Extension
        } else {
            Era::Modern
        }
    }
}

/// Slope (rise over run) per cell.
fn slopes(land: &Land) -> Vec<f32> {
    let g = &land.height;
    let mut out = vec![0.0; g.w * g.h];
    for j in 0..g.h {
        for i in 0..g.w {
            let hx = *g.at((i + 1).min(g.w - 1), j) - *g.at(i.saturating_sub(1), j);
            let hy = *g.at(i, (j + 1).min(g.h - 1)) - *g.at(i, j.saturating_sub(1));
            out[j * g.w + i] = (hx * hx + hy * hy).sqrt() / (2.0 * g.cell as f32);
        }
    }
    out
}

/// Least total cost from the sources over 8-connected cells, where a step
/// costs its length times the mean cost of its two cells. Returns the cost
/// per cell and the cell each was reached from.
fn dijkstra<T: Clone>(
    g: &Grid<T>,
    cost: &[f32],
    sources: &[((usize, usize), f64)],
) -> (Vec<f32>, Vec<Option<u32>>) {
    let (w, h) = (g.w, g.h);
    let mut dist = vec![f32::INFINITY; w * h];
    let mut via = vec![None; w * h];
    let mut heap = BinaryHeap::new();
    for ((i, j), head) in sources {
        let k = j * w + i;
        if cost[k].is_finite() && (*head as f32) < dist[k] {
            dist[k] = *head as f32;
            heap.push(Reverse((Key(*head as f32), k as u32)));
        }
    }
    let c = g.cell as f32;
    let steps: [(i64, i64, f32); 8] = [
        (1, 0, c),
        (-1, 0, c),
        (0, 1, c),
        (0, -1, c),
        (1, 1, c * std::f32::consts::SQRT_2),
        (1, -1, c * std::f32::consts::SQRT_2),
        (-1, 1, c * std::f32::consts::SQRT_2),
        (-1, -1, c * std::f32::consts::SQRT_2),
    ];
    while let Some(Reverse((Key(d), k))) = heap.pop() {
        let k = k as usize;
        if d > dist[k] {
            continue;
        }
        let (i, j) = ((k % w) as i64, (k / w) as i64);
        for (di, dj, len) in steps {
            let (ni, nj) = (i + di, j + dj);
            if ni < 0 || nj < 0 || ni >= w as i64 || nj >= h as i64 {
                continue;
            }
            let nk = nj as usize * w + ni as usize;
            let step = len * 0.5 * (cost[k] + cost[nk]);
            let nd = d + step;
            if nd < dist[nk] {
                dist[nk] = nd;
                via[nk] = Some(k as u32);
                heap.push(Reverse((Key(nd), nk as u32)));
            }
        }
    }
    (dist, via)
}

/// A total order on f32 for the heap (costs are never NaN).
#[derive(Clone, Copy, PartialEq)]
struct Key(f32);
impl Eq for Key {}
impl PartialOrd for Key {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Key {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&o.0)
    }
}

/// Where an old road heading `dir` leaves: the reachable land
/// cell most nearly that way on the map's edge, or on a shore (a ferry) when
/// the land ends first.
fn edge_cell(land: &Land, dir: DVec2, via: &[Option<u32>]) -> Option<(usize, usize)> {
    let g = &land.cover;
    let (w, h) = (g.w, g.h);
    let mut best: Option<(f64, (usize, usize))> = None;
    for j in 0..h {
        for i in 0..w {
            if g.at(i, j).is_water() || via[j * w + i].is_none() && (i, j) != (w / 2, h / 2) {
                continue;
            }
            let edge = i == 0 || j == 0 || i == w - 1 || j == h - 1;
            let shore = !edge
                && [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .any(|(di, dj)| {
                        matches!(
                            g.at((i as i64 + di) as usize, (j as i64 + dj) as usize),
                            Cover::Sea | Cover::Lake
                        )
                    });
            if !edge && !shore {
                continue;
            }
            let q = g.center(i, j);
            let len = q.length();
            let cos = if len < 1e-9 { -1.0 } else { q.dot(dir) / len };
            if cos < 0.8 {
                continue;
            }
            // The map edge straight ahead; a ferry only where the land ends.
            let score = if edge { cos } else { cos - 0.5 };
            if best.is_none_or(|b| score > b.0) {
                best = Some((score, (i, j)));
            }
        }
    }
    best.map(|b| b.1)
}

/// The path of cell centres from the source to `cell`, source first.
fn trace<T: Clone>(g: &Grid<T>, via: &[Option<u32>], cell: (usize, usize)) -> Vec<DVec2> {
    let mut k = cell.1 * g.w + cell.0;
    let mut out = vec![g.center(cell.0, cell.1)];
    while let Some(from) = via[k] {
        k = from as usize;
        out.push(g.center(k % g.w, k / g.w));
    }
    out.reverse();
    out
}

/// The tree of paths from the core to the gates, as smooth pieces between
/// forks.
fn branches<T: Clone>(
    g: &Grid<T>,
    via: &[Option<u32>],
    gates: &[DVec2],
    core: (usize, usize),
) -> Vec<Vec<DVec2>> {
    use std::collections::BTreeMap;
    let mut children: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut ends = std::collections::BTreeSet::new();
    for q in gates {
        let (i, j) = g.cell_of(*q).expect("gate on the map");
        let mut k = j * g.w + i;
        ends.insert(k);
        while let Some(from) = via[k] {
            let from = from as usize;
            let c = children.entry(from).or_default();
            if c.contains(&k) {
                break;
            }
            c.push(k);
            k = from;
        }
    }
    let center = |k: usize| g.center(k % g.w, k / g.w);
    let mut out = Vec::new();
    let mut stack = vec![core.1 * g.w + core.0];
    while let Some(start) = stack.pop() {
        for &first in children.get(&start).map(|v| v.as_slice()).unwrap_or(&[]) {
            let mut path = vec![center(start)];
            let mut k = first;
            loop {
                path.push(center(k));
                match children.get(&k).map(|v| v.as_slice()) {
                    Some([only]) if !ends.contains(&k) => k = *only,
                    Some(_) => {
                        stack.push(k);
                        break;
                    }
                    None => break,
                }
            }
            out.push(chaikin(&simplify(&path, g.cell * 1.2), 3));
        }
    }
    out
}

/// Corner cutting, keeping both ends.
pub fn chaikin(pts: &[DVec2], rounds: usize) -> Vec<DVec2> {
    let mut p = pts.to_vec();
    for _ in 0..rounds {
        if p.len() < 3 {
            return p;
        }
        let mut q = vec![p[0]];
        for w in p.windows(2) {
            q.push(w[0].lerp(w[1], 0.25));
            q.push(w[0].lerp(w[1], 0.75));
        }
        q[1] = p[0].lerp(p[1], 0.5);
        let n = q.len();
        q[n - 1] = p[p.len() - 2].lerp(p[p.len() - 1], 0.5);
        q.push(*p.last().expect("non-empty"));
        p = q;
    }
    p
}

/// Douglas-Peucker simplification to within `tol`.
pub fn simplify(pts: &[DVec2], tol: f64) -> Vec<DVec2> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut stack = vec![(0, pts.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let mut far = (0.0, a);
        for k in a + 1..b {
            let d = crate::water::segment_distance(pts[k], pts[a], pts[b]);
            if d > far.0 {
                far = (d, k);
            }
        }
        if far.0 > tol {
            keep[far.1] = true;
            stack.push((a, far.1));
            stack.push((far.1, b));
        }
    }
    pts.iter()
        .zip(keep)
        .filter(|(_, k)| *k)
        .map(|(p, _)| *p)
        .collect()
}

/// The point `d` metres along a polyline.
fn point_along(pts: &[DVec2], mut d: f64) -> Option<DVec2> {
    for w in pts.windows(2) {
        let l = w[0].distance(w[1]);
        if d <= l {
            return Some(w[0].lerp(w[1], d / l.max(1e-9)));
        }
        d -= l;
    }
    None
}

/// Marks the cells within `r` of a polyline.
fn stamp_line<T: Clone>(g: &Grid<T>, pts: &[DVec2], r: f64, out: &mut [bool]) {
    for w in pts.windows(2) {
        let lo = w[0].min(w[1]) - DVec2::splat(r);
        let hi = w[0].max(w[1]) + DVec2::splat(r);
        let i0 = (((lo.x - g.origin.x) / g.cell).floor().max(0.0)) as usize;
        let j0 = (((lo.y - g.origin.y) / g.cell).floor().max(0.0)) as usize;
        let i1 = (((hi.x - g.origin.x) / g.cell).ceil() as usize).min(g.w - 1);
        let j1 = (((hi.y - g.origin.y) / g.cell).ceil() as usize).min(g.h - 1);
        for j in j0..=j1 {
            for i in i0..=i1 {
                if crate::water::segment_distance(g.center(i, j), w[0], w[1]) <= r {
                    out[j * g.w + i] = true;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn towns_hold_their_people_and_are_not_round() {
        let mut roundness = Vec::new();
        for s in 0..12 {
            let pop = det::powf(10.0, 2.5 + (s % 4) as f64);
            let p = TownParams::sample(Seed(s), pop);
            let land = Land::generate(Seed(s), &p);
            let gr = Growth::generate(Seed(s), &p, &land);
            let g = &gr.time;
            let mut built = 0usize;
            let mut far: f64 = 0.0;
            for j in 0..g.h {
                for i in 0..g.w {
                    if gr.built(i, j) {
                        built += 1;
                        assert!(!land.cover.at(i, j).is_water());
                        far = far.max(g.center(i, j).length());
                    }
                }
            }
            let area = built as f64 * g.cell * g.cell;
            let ratio = area / p.built_area();
            assert!((0.8..1.3).contains(&ratio), "seed {s}: area ratio {ratio}");
            assert!(!gr.roads.is_empty(), "seed {s}: no roads");
            // Area against the disc reaching its farthest cell.
            roundness.push(area / (std::f64::consts::PI * far * far));
        }
        roundness.sort_by(f64::total_cmp);
        let median = roundness[roundness.len() / 2];
        assert!(median < 0.6, "towns too round: median fill {median}");
    }
}
