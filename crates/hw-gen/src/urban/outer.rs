//! The town's rivers beyond the town: on a planet a river does not end at
//! the edge of the planned land. It carries on upstream along the lowest
//! ground, narrowing, to its head in the hills, and cuts its valley into the
//! planet's ground the same way the town's land falls to its water.

use super::land::{smooth, River};
use crate::planet::PlanetPatch;
use crate::water::segment_distance;
use glam::DVec2;
use hw_core::det;
use std::collections::BTreeMap;

/// A river stops climbing where the land around it stands this high (m):
/// all water lies at sea level for now, so beyond this it would be a gorge.
const HEAD_GROUND: f64 = 150.0;
/// The longest stretch a river carries on past the town (m).
const MAX_REACH: f64 = 25_000.0;
const STEP: f64 = 150.0;
const CELL: f64 = 512.0;

/// The rivers of a town on a planet, each from its head in the hills down
/// through the town, with what is needed to cut their valleys.
#[derive(Clone, Debug, Default)]
pub struct OuterRivers {
    /// Centre lines (points about [`STEP`] apart) with the width at each point.
    pub rivers: Vec<River>,
    /// How far the valley sides take to reach the land's own height (m).
    rise: f64,
    /// Segments by grid cell: (river, segment).
    index: BTreeMap<(i64, i64), Vec<(u32, u32)>>,
}

impl OuterRivers {
    /// Carry the town's `rivers` (upstream end first) on over the planet.
    pub fn new(rivers: &[River], planet: &PlanetPatch, relief: f64) -> Self {
        let rise = 120.0 + relief * 500.0;
        let mut out = OuterRivers {
            rivers: Vec::new(),
            rise,
            index: BTreeMap::new(),
        };
        for r in rivers {
            if r.pts.len() < 2 {
                continue;
            }
            // The town's stretch, thinned to one point every STEP or so.
            let mut pts = vec![r.pts[0]];
            let mut widths = vec![r.widths[0]];
            for (q, w) in r.pts.iter().zip(&r.widths).skip(1) {
                if q.distance(*pts.last().expect("a point")) >= STEP {
                    pts.push(*q);
                    widths.push(*w);
                }
            }
            // Upstream from the town: the lowest way that leads away from it.
            let w0 = r.widths[0];
            let mut p = r.pts[0];
            let mut heading = {
                let d = r.pts[0] - r.pts[1.min(r.pts.len() - 1)];
                det::atan2(d.y, d.x)
            };
            let (mut up, mut up_w) = (Vec::new(), Vec::new());
            let mut s = 0.0;
            while s < MAX_REACH {
                let mut best: Option<(f64, f64)> = None;
                for k in -3i32..=3 {
                    let a = heading + k as f64 * 0.28;
                    let q = p + det::dir2(a) * STEP * 2.0;
                    if q.length() <= p.length() {
                        continue;
                    }
                    let cost = planet.ground(q) + (k * k) as f64 * 0.4;
                    if best.is_none_or(|b| cost < b.0) {
                        best = Some((cost, a));
                    }
                }
                let Some((_, a)) = best else { break };
                heading = a;
                p += det::dir2(a) * STEP;
                s += STEP;
                let g = planet.ground(p);
                if !(0.0..HEAD_GROUND).contains(&g) {
                    break;
                }
                up.push(p);
                up_w.push(w0);
            }
            // Narrowing to a brook at its head.
            let n = up.len();
            for (k, w) in up_w.iter_mut().enumerate() {
                let t = (k + 1) as f64 / n.max(1) as f64;
                *w = (w0 * (1.0 - 0.85 * t)).max(3.0);
            }
            up.reverse();
            up_w.reverse();
            up.extend(pts);
            up_w.extend(widths);
            out.rivers.push(River {
                pts: up,
                widths: up_w,
            });
        }
        let reach = rise.max(3.0 * HEAD_GROUND * 2.0) + 60.0;
        for (ri, r) in out.rivers.iter().enumerate() {
            for (si, seg) in r.pts.windows(2).enumerate() {
                let half = r.widths[si].max(r.widths[si + 1]) * 0.5 + reach;
                let lo = seg[0].min(seg[1]) - DVec2::splat(half);
                let hi = seg[0].max(seg[1]) + DVec2::splat(half);
                for cx in (lo.x / CELL).floor() as i64..=(hi.x / CELL).floor() as i64 {
                    for cy in (lo.y / CELL).floor() as i64..=(hi.y / CELL).floor() as i64 {
                        out.index
                            .entry((cx, cy))
                            .or_default()
                            .push((ri as u32, si as u32));
                    }
                }
            }
        }
        out
    }

    /// The planet's `ground` at `p` with the rivers' valleys cut into it.
    pub fn carve(&self, p: DVec2, ground: f64) -> f64 {
        let key = ((p.x / CELL).floor() as i64, (p.y / CELL).floor() as i64);
        let Some(segs) = self.index.get(&key) else {
            return ground;
        };
        // Distance beyond the water's edge of the nearest river (negative in it).
        let mut edge = f64::MAX;
        for &(ri, si) in segs {
            let r = &self.rivers[ri as usize];
            let (si, a, b) = (si as usize, r.pts[si as usize], r.pts[si as usize + 1]);
            let d = segment_distance(p, a, b);
            let len = a.distance(b).max(1e-6);
            let t = ((p - a).dot(b - a) / (len * len)).clamp(0.0, 1.0);
            let w = r.widths[si] + (r.widths[si + 1] - r.widths[si]) * t;
            edge = edge.min(d - w * 0.5);
        }
        if edge <= 0.0 {
            // The bed, deeper towards the middle.
            return ground.min(-(1.5 + (-edge * 0.12).min(12.0)));
        }
        if ground <= 0.0 {
            return ground;
        }
        // Banks a little above the water, rising to the land's own height.
        // The sides are no steeper than 1 in 3.
        let reach = self.rise.max(3.0 * ground);
        ground.min(1.0 + 2.0 * smooth(0.0, 40.0, edge) + ground * smooth(0.0, reach, edge))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planet::{choose_home, PlanetMap};
    use crate::urban::{params::TownParams, world::TownWorld};
    use hw_core::Seed;

    #[test]
    fn a_river_carries_on_past_the_town_in_water_all_the_way() {
        let seed = Seed(1);
        let map = PlanetMap::build(seed, 96);
        let patch = std::sync::Arc::new(PlanetPatch::new(&map, choose_home(&map)));
        let mut params = TownParams::on_planet(seed, 20_000.0, &patch);
        params.rivers = params.rivers.max(1);
        let town = TownWorld::generate_on_or_stop(seed, params, None, &hw_core::Cancel::new())
            .expect("not cancelled");
        let outer = town.outer_rivers().expect("on a planet");
        assert!(!outer.rivers.is_empty());
        for r in &outer.rivers {
            // Under water along the whole centre line, town and country alike:
            // no dam where the town's land ends.
            for q in &r.pts {
                let g = town.ground(*q);
                assert!(g < 0.0, "dry river bed ({g} m) at {q}");
            }
            // And dry land not far to the side of it, upstream of the town.
            let (a, b) = (r.pts[0], r.pts[1]);
            let side = (b - a).normalize().perp() * 900.0;
            assert!(town.ground(a + side) > 0.0 || town.ground(a - side) > 0.0);
        }
    }
}
