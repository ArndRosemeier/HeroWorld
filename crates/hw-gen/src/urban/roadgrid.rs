//! A 32 m grid over the town's ground plane, and the road segments filed in it,
//! to ask quickly what is near a point.

use super::world::TownWorld;
use crate::water::segment_distance;
use glam::DVec2;
use std::collections::BTreeMap;

/// A grid cell.
pub(super) type Key = (i64, i64);
const GRID: f64 = 32.0;

/// The cell holding `p`.
pub(super) fn key(p: DVec2) -> Key {
    ((p.x / GRID).floor() as i64, (p.y / GRID).floor() as i64)
}

/// The cells a square of half-size `r` around `p` touches.
pub(super) fn keys_around(p: DVec2, r: f64) -> impl Iterator<Item = Key> {
    let (a, b) = (key(p - DVec2::splat(r)), key(p + DVec2::splat(r)));
    (a.0..=b.0).flat_map(move |x| (a.1..=b.1).map(move |z| (x, z)))
}

/// Road segments by grid cell: each segment is filed under every cell that
/// its carriageway and pavements, widened by a margin, may reach.
pub(super) struct RoadGrid<'a> {
    w: &'a TownWorld,
    cells: BTreeMap<Key, Vec<(usize, usize)>>,
}

impl<'a> RoadGrid<'a> {
    pub(super) fn new(w: &'a TownWorld, margin: f64) -> Self {
        let mut cells: BTreeMap<Key, Vec<(usize, usize)>> = BTreeMap::new();
        for (ri, r) in w.roads.iter().enumerate() {
            let reach = r.half + r.pavement + margin;
            for (si, s) in r.pts.windows(2).enumerate() {
                let (a, b) = (
                    key(s[0].min(s[1]) - DVec2::splat(reach)),
                    key(s[0].max(s[1]) + DVec2::splat(reach)),
                );
                for x in a.0..=b.0 {
                    for z in a.1..=b.1 {
                        cells.entry((x, z)).or_default().push((ri, si));
                    }
                }
            }
        }
        RoadGrid { w, cells }
    }

    /// The segments filed under cell `k`, as (road, segment).
    pub(super) fn at(&self, k: Key) -> &[(usize, usize)] {
        self.cells.get(&k).map_or(&[], |l| l.as_slice())
    }

    /// Does a disc of radius `r` at `p` reach onto any road but `skip`,
    /// pavements included?
    pub(super) fn touches(&self, p: DVec2, r: f64, skip: Option<usize>) -> bool {
        keys_around(p, r).any(|k| {
            self.at(k).iter().any(|&(ri, si)| {
                let road = &self.w.roads[ri];
                Some(ri) != skip
                    && segment_distance(p, road.pts[si], road.pts[si + 1])
                        < road.half + road.pavement + r
            })
        })
    }
}
