//! The streets wide enough for the colossus, as a graph it walks along.
//!
//! The walking tank is about ten metres across, so it keeps to main roads at
//! least [`TANK_STREET`] wide from wall to wall. Each such road becomes a
//! chain of nodes a few metres apart; where two roads meet their nodes are
//! linked. A route is the shortest chain of nodes between the ones nearest
//! two points (A*, ties broken by node index, so every machine walks the same
//! way).

use glam::DVec2;
use hw_gen::urban::world::TownWorld;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

/// Narrowest road (carriageway and pavements, m) the tank walks down.
pub const TANK_STREET: f64 = 11.0;
/// Node spacing along a road (m).
const STEP: f64 = 6.0;
/// Nodes of two roads this near are one crossing (m).
const JOIN: f64 = 5.0;
/// Spatial cell (m).
const CELL: f64 = 16.0;

type Key = (i64, i64);

fn key(p: DVec2) -> Key {
    ((p.x / CELL).floor() as i64, (p.y / CELL).floor() as i64)
}

/// The walkable street graph.
#[derive(Debug, Default)]
pub struct Streets {
    nodes: Vec<DVec2>,
    links: Vec<Vec<u32>>,
    cells: BTreeMap<Key, Vec<u32>>,
}

impl Streets {
    /// The main roads of `world` at least `width` across.
    pub fn new(world: &TownWorld, width: f64) -> Self {
        let mut s = Streets::default();
        // The road each node lies on.
        let mut road_of: Vec<u32> = Vec::new();
        for r in world.roads.iter().filter(|r| r.main) {
            if (r.half + r.pavement) * 2.0 < width || r.pts.len() < 2 {
                continue;
            }
            let mut last: Option<u32> = None;
            let mut prev = r.pts[0];
            let add = |s: &mut Streets, p: DVec2, last: &mut Option<u32>| {
                let n = s.nodes.len() as u32;
                s.nodes.push(p);
                s.links.push(Vec::new());
                s.cells.entry(key(p)).or_default().push(n);
                if let Some(l) = *last {
                    s.link(l, n);
                }
                *last = Some(n);
            };
            let id = road_of.last().map_or(0, |r| r + 1);
            add(&mut s, prev, &mut last);
            for &q in &r.pts[1..] {
                let len = prev.distance(q);
                let n = (len / STEP).ceil().max(1.0) as usize;
                for k in 1..=n {
                    add(&mut s, prev.lerp(q, k as f64 / n as f64), &mut last);
                }
                prev = q;
            }
            road_of.resize(s.nodes.len(), id);
        }
        // Crossings: join the nearest node of every other road nearby.
        for i in 0..s.nodes.len() {
            let p = s.nodes[i];
            let mut best: BTreeMap<u32, (f64, u32)> = BTreeMap::new();
            for j in s.near(p, JOIN) {
                let (ri, rj) = (road_of[i], road_of[j as usize]);
                if ri == rj {
                    continue;
                }
                let d = p.distance(s.nodes[j as usize]);
                let e = best.entry(rj).or_insert((d, j));
                if d < e.0 || (d == e.0 && j < e.1) {
                    *e = (d, j);
                }
            }
            for (_, (_, j)) in best {
                s.link(i as u32, j);
            }
        }
        s
    }

    fn link(&mut self, a: u32, b: u32) {
        if a != b && !self.links[a as usize].contains(&b) {
            self.links[a as usize].push(b);
            self.links[b as usize].push(a);
        }
    }

    /// Nodes within `r` of `p`, in index order.
    fn near(&self, p: DVec2, r: f64) -> Vec<u32> {
        let (a, b) = (key(p - DVec2::splat(r)), key(p + DVec2::splat(r)));
        let mut out: Vec<u32> = (a.0..=b.0)
            .flat_map(|x| (a.1..=b.1).map(move |z| (x, z)))
            .filter_map(|k| self.cells.get(&k))
            .flatten()
            .copied()
            .filter(|&n| self.nodes[n as usize].distance(p) <= r)
            .collect();
        out.sort_unstable();
        out
    }

    /// Is there anywhere to walk?
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The node nearest `p`.
    pub fn nearest(&self, p: DVec2) -> Option<u32> {
        let mut r = CELL;
        while r < 4096.0 {
            let best = self.near(p, r).into_iter().min_by(|&a, &b| {
                let (da, db) = (
                    self.nodes[a as usize].distance(p),
                    self.nodes[b as usize].distance(p),
                );
                da.total_cmp(&db).then(a.cmp(&b))
            });
            if best.is_some() {
                return best;
            }
            r *= 2.0;
        }
        (0..self.nodes.len() as u32).min_by(|&a, &b| {
            self.nodes[a as usize]
                .distance(p)
                .total_cmp(&self.nodes[b as usize].distance(p))
                .then(a.cmp(&b))
        })
    }

    /// Node `n`'s position.
    pub fn node(&self, n: u32) -> DVec2 {
        self.nodes[n as usize]
    }

    /// Number of nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// The street points from the node nearest `from` to the one nearest
    /// `to`, both included; None if the streets do not join them.
    pub fn route(&self, from: DVec2, to: DVec2) -> Option<Vec<DVec2>> {
        let (a, b) = (self.nearest(from)?, self.nearest(to)?);
        let goal = self.nodes[b as usize];
        let bits = |d: f64| (d * 1000.0) as u64;
        let mut cost: BTreeMap<u32, f64> = BTreeMap::new();
        let mut came: BTreeMap<u32, u32> = BTreeMap::new();
        let mut open = BinaryHeap::new();
        cost.insert(a, 0.0);
        open.push(Reverse((bits(self.nodes[a as usize].distance(goal)), a)));
        while let Some(Reverse((_, n))) = open.pop() {
            if n == b {
                let mut path = vec![goal];
                let mut k = b;
                while let Some(&p) = came.get(&k) {
                    path.push(self.nodes[p as usize]);
                    k = p;
                }
                path.reverse();
                return Some(path);
            }
            let here = cost[&n];
            for &m in &self.links[n as usize] {
                let c = here + self.nodes[n as usize].distance(self.nodes[m as usize]);
                if cost.get(&m).is_none_or(|&old| c < old) {
                    cost.insert(m, c);
                    came.insert(m, n);
                    let h = self.nodes[m as usize].distance(goal);
                    open.push(Reverse((bits(c + h), m)));
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hw_core::Seed;

    #[test]
    fn the_tank_finds_its_way_along_main_roads() {
        let t = crate::GeneratedTown::generate(Seed(1), crate::DEFAULT_POPULATION);
        let s = Streets::new(&t.world, TANK_STREET);
        assert!(s.len() > 100, "{} nodes", s.len());
        let mut comp = vec![u32::MAX; s.len()];
        let mut sizes = Vec::new();
        for i in 0..s.len() {
            if comp[i] != u32::MAX {
                continue;
            }
            let c = sizes.len() as u32;
            let mut stack = vec![i as u32];
            let mut n = 0;
            comp[i] = c;
            while let Some(k) = stack.pop() {
                n += 1;
                for &m in &s.links[k as usize] {
                    if comp[m as usize] == u32::MAX {
                        comp[m as usize] = c;
                        stack.push(m);
                    }
                }
            }
            sizes.push(n);
        }
        // Nearly all of it is one network.
        let big = (0..sizes.len()).max_by_key(|&c| sizes[c]).unwrap();
        assert!(sizes[big] * 10 > s.len() * 9, "{sizes:?}");
        let ends: Vec<u32> = (0..s.len() as u32)
            .filter(|&n| comp[n as usize] == big as u32)
            .collect();
        let (a, b) = (s.node(ends[0]), s.node(*ends.last().unwrap()));
        let r = s.route(a, b).expect("a route");
        assert!(r.len() > 2);
        for w in r.windows(2) {
            assert!(w[0].distance(w[1]) <= STEP + JOIN + 0.01, "{w:?}");
        }
        assert_eq!(s.route(a, b), Some(r));
    }
}
