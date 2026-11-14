//! The town's rivers beyond the town: on a planet a river does not end at
//! the edge of the planned land. It carries on upstream along the lowest
//! ground, narrowing, to its head in the hills, and cuts its bed into the
//! planet's ground.
//!
//! Beyond the town a river climbs with the land as a flight of pools. Each
//! pool is level; at its lower end the water runs over a sill and falls to
//! the next pool. Gentle country gives long pools and falls of a few metres,
//! steep country a cascade, and here and there a pool widens into a lake.
//! Inside the town (and wherever the river has not yet left sea level) the
//! water stands at the sea's level as before.

use super::land::{smooth, River};
use crate::water::segment_distance;
use glam::DVec2;
use hw_core::{det, Seed};
use std::collections::BTreeMap;

/// A river stops climbing where the land stands this high (m).
const HEAD_GROUND: f64 = 2500.0;
/// The longest stretch a river carries on past the town (m).
const MAX_REACH: f64 = 25_000.0;
const STEP: f64 = 150.0;
const CELL: f64 = 512.0;
/// The narrowest a river gets (m): its head is a brook.
const BROOK: f64 = 12.0;
/// A pool's surface lies this far under the land the river runs through (m).
const CUT: f64 = 1.0;
/// The tallest single fall (m): where the land rises faster, the river
/// climbs it as a cascade in a gorge.
const TALLEST_FALL: f64 = 18.0;
/// The deepest gorge (m): land that stands higher than this over the river
/// ends it, and its spring rises at the foot of the wall.
const DEEPEST_GORGE: f64 = 150.0;
/// The river's head is where the land stops rising: once the ground has
/// come down this far from the highest the river has reached (m), it has
/// crossed into the next valley.
const OVER_THE_DIVIDE: f64 = 1.5;
/// The sill at a pool's lower end: how far it reaches upstream of the fall,
/// how wide the notch is that the water runs through, and how deep the water
/// stands in the notch (m).
const SILL: f64 = 12.0;
const NOTCH: f64 = 12.0;
const SILL_DEPTH: f64 = 0.35;
/// Banks reach at most this far from the water (m).
const FAR_BANK: f64 = 900.0;
/// A canyon through a mesa: flat ground this far from the water (m), and the
/// walls at their full height this far.
const CANYON_FLOOR: f64 = 45.0;
const CANYON_RIM: f64 = 190.0;

/// The rivers of a town on a planet, each from its head in the hills down
/// through the town, with what is needed to cut their beds.
#[derive(Clone, Debug, Default)]
pub struct OuterRivers {
    /// Centre lines (points about [`STEP`] apart) with the width at each point.
    pub rivers: Vec<River>,
    /// The still level of the water at each point of each river (m): equal
    /// along a pool, lower from one pool to the next downstream, 0 in town.
    /// Where two points differ the river falls halfway between them.
    pub levels: Vec<Vec<f64>>,
    /// How far the valley sides take to reach the land's own height (m).
    rise: f64,
    /// Segments by grid cell: (river, segment).
    index: BTreeMap<(i64, i64), Vec<(u32, u32)>>,
}

/// A spring: where a river rises, and how much water (m³/s).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    pub at: DVec2,
    /// The level of the pool it feeds.
    pub level: f64,
    pub flow: f64,
}

/// Where a point lies by the nearest river.
struct Near {
    /// Distance beyond the water's edge (negative in the water).
    edge: f64,
    /// Distance from the centre line.
    off: f64,
    /// Level of the water here.
    level: f64,
    /// The banks' level here: the water's, eased down past a fall.
    bank: f64,
    /// Metres upstream of a fall's line, if within its sill.
    sill: Option<f64>,
    /// The river's width here.
    width: f64,
}

impl OuterRivers {
    /// Carry the town's `rivers` (upstream end first) on over the country,
    /// whose height at a point is `ground`. `half` is the half-width of the
    /// town's own land, where the water stays at sea level.
    pub fn new(
        seed: Seed,
        rivers: &[River],
        ground: &dyn Fn(DVec2) -> f64,
        relief: f64,
        half: f64,
    ) -> Self {
        let rise = 120.0 + relief * 500.0;
        let mut out = OuterRivers {
            rivers: Vec::new(),
            levels: Vec::new(),
            rise,
            index: BTreeMap::new(),
        };
        for (ri, r) in rivers.iter().enumerate() {
            if r.pts.len() < 2 {
                continue;
            }
            let seed = seed.child("outer-river").child(ri as u64);
            // The town's stretch, thinned to one point every STEP or so.
            let mut pts = vec![r.pts[0]];
            let mut widths = vec![r.widths[0]];
            for (q, w) in r.pts.iter().zip(&r.widths).skip(1) {
                if q.distance(*pts.last().expect("a point")) >= STEP {
                    pts.push(*q);
                    widths.push(*w);
                }
            }
            // Upstream from the town: the lowest way that leads away from it,
            // climbing pool by pool.
            let w0 = r.widths[0];
            let mut p = r.pts[0];
            let mut heading = {
                let d = r.pts[0] - r.pts[1.min(r.pts.len() - 1)];
                det::atan2(d.y, d.x)
            };
            let (mut up, mut up_w, mut up_l) = (Vec::new(), Vec::new(), Vec::new());
            let mut s = 0.0;
            let mut level = 0.0;
            let mut highest = f64::MIN;
            while s < MAX_REACH {
                let mut best: Option<(f64, f64)> = None;
                for k in -3i32..=3 {
                    let a = heading + k as f64 * 0.28;
                    let q = p + det::dir2(a) * STEP * 2.0;
                    if q.length() <= p.length() {
                        continue;
                    }
                    let cost = ground(q) + (k * k) as f64 * 0.4;
                    if best.is_none_or(|b| cost < b.0) {
                        best = Some((cost, a));
                    }
                }
                let Some((_, a)) = best else { break };
                heading = a;
                p += det::dir2(a) * STEP;
                s += STEP;
                let g = ground(p);
                if !(0.0..HEAD_GROUND).contains(&g) || g < highest - OVER_THE_DIVIDE {
                    break;
                }
                highest = highest.max(g);
                // The next pool, once the land has risen a fall's height
                // above this one (never inside the town's own land).
                let outside = p.x.abs().max(p.y.abs()) > half * 1.03;
                let fall = 1.5 + 2.5 * seed.child("fall").child(up.len() as u64).unit();
                let target = g - CUT;
                if outside && target - level >= fall {
                    if target - level > DEEPEST_GORGE {
                        break;
                    }
                    let tallest = 5.0
                        + (TALLEST_FALL - 5.0) * seed.child("tall").child(up.len() as u64).unit();
                    level += (target - level).min(tallest);
                    level = (level * 4.0).round() / 4.0;
                }
                up.push(p);
                up_w.push(w0);
                up_l.push(level);
            }
            // Narrowing to a brook at its head.
            let n = up.len();
            for (k, w) in up_w.iter_mut().enumerate() {
                let t = (k + 1) as f64 / n.max(1) as f64;
                *w = (w0 * (1.0 - 0.85 * t)).max(BROOK.min(w0));
            }
            // Here and there a long pool widens into a lake.
            let mut k = 0;
            while k < n {
                let mut e = k;
                while e + 1 < n && up_l[e + 1] == up_l[k] {
                    e += 1;
                }
                let pool = seed.child("lake").child(k as u64);
                if up_l[k] > 0.0 && e - k >= 3 && pool.unit() < 0.45 {
                    let extra = 120.0 + 380.0 * pool.child("size").unit();
                    for (j, w) in up_w.iter_mut().enumerate().take(e + 1).skip(k) {
                        // Widest in the middle, the river's own width at both ends.
                        let u = (j - k) as f64 / (e - k) as f64;
                        let bulge = det::sin(u * std::f64::consts::PI);
                        // And never so wide so soon that its round end
                        // would reach past the pool's own, into the next.
                        let ends = (j - k).min(e - j) as f64 * STEP;
                        *w += (extra * bulge * bulge).min(1.2 * ends);
                    }
                }
                k = e + 1;
            }
            up.reverse();
            up_w.reverse();
            up_l.reverse();
            up_l.extend(std::iter::repeat_n(0.0, pts.len()));
            up.extend(pts);
            up_w.extend(widths);
            out.rivers.push(River {
                pts: up,
                widths: up_w,
            });
            out.levels.push(up_l);
        }
        let reach = FAR_BANK + 60.0;
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

    /// Where `p` lies by the nearest river, if one is near.
    fn near(&self, p: DVec2) -> Option<Near> {
        let key = ((p.x / CELL).floor() as i64, (p.y / CELL).floor() as i64);
        let segs = self.index.get(&key)?;
        let mut best: Option<Near> = None;
        for &(ri, si) in segs {
            let r = &self.rivers[ri as usize];
            let levels = &self.levels[ri as usize];
            let (si, a, b) = (si as usize, r.pts[si as usize], r.pts[si as usize + 1]);
            let off = segment_distance(p, a, b);
            let len = a.distance(b).max(1e-6);
            let t = ((p - a).dot(b - a) / (len * len)).clamp(0.0, 1.0);
            let w = r.widths[si] + (r.widths[si + 1] - r.widths[si]) * t;
            let edge = off - w * 0.5;
            if best.as_ref().is_some_and(|n| n.edge <= edge) {
                continue;
            }
            // The fall, if this stretch has one, lies about halfway along it:
            // a horseshoe that reaches furthest upstream in midstream, with
            // a ragged lip.
            let (hi, lo) = (levels[si], levels[si + 1]);
            let (level, bank, sill) = if hi == lo {
                (hi, hi, None)
            } else {
                let side = (p - a).perp_dot(b - a) / len;
                let across = (side / (w * 0.5)).clamp(-1.3, 1.3);
                let shoe = (w * 0.22).min(26.0) * (1.0 - across * across);
                let ragged = 2.2 * det::sin(side * 0.31 + hi) + 1.3 * det::sin(side * 0.83 + lo);
                let past = (t - 0.5) * len + shoe + ragged;
                // By the water the banks step down with it, over a stretch
                // that grows with the fall; further off they follow the
                // river's even slope, so the valley sides are not terraced.
                let even = hi + (lo - hi) * t;
                let far = smooth(0.0, 60.0, edge);
                if past < 0.0 {
                    (hi, hi + (even - hi) * far, (-past <= SILL).then_some(-past))
                } else {
                    let ease = ((hi - lo) * 0.6).clamp(6.0, 40.0);
                    let stepped = hi + (lo - hi) * smooth(0.0, ease, past);
                    (lo, stepped + (even - stepped) * far, None)
                }
            };
            best = Some(Near {
                edge,
                off,
                level,
                bank,
                sill,
                width: w,
            });
        }
        best
    }

    /// How much of a mesa stands at `p`: none by a river, all of it
    /// [`CANYON_RIM`] from the water. Rivers run on the valley floor; where
    /// a tableland lies across one, it is cut back from the water and the
    /// river runs through it in a canyon.
    pub fn canyon(&self, p: DVec2) -> f64 {
        self.near(p)
            .map_or(1.0, |n| smooth(CANYON_FLOOR, CANYON_RIM, n.edge))
    }

    /// The planet's `ground` at `p` with the rivers' beds and banks cut into it.
    pub fn carve(&self, p: DVec2, ground: f64) -> f64 {
        let Some(n) = self.near(p) else {
            return ground;
        };
        if n.level <= 0.0 && n.bank <= 0.0 {
            // At sea level, as the town's own water.
            if n.edge <= 0.0 {
                // The bed, deeper towards the middle.
                return ground.min(-(1.5 + (-n.edge * 0.12).min(12.0)));
            }
            if ground <= 0.0 {
                return ground;
            }
            // Banks a little above the water, rising to the land's own height.
            // The sides are no steeper than 1 in 3.
            let reach = self.rise.max(3.0 * ground);
            return ground
                .min(1.0 + 2.0 * smooth(0.0, 40.0, n.edge) + ground * smooth(0.0, reach, n.edge));
        }
        if n.edge <= 0.0 {
            if n.sill.is_some() {
                // The sill: rock just above the pool, with a notch the water
                // runs through to the fall.
                // A brook pours through a narrow notch, a broad river over
                // close to half its width.
                let notch = NOTCH.max(n.width * 0.45);
                return if n.off < notch * 0.5 {
                    n.level - SILL_DEPTH
                } else {
                    n.level + 0.5
                };
            }
            // The bed: shallow at the edge, deeper towards the middle.
            return ground.min(n.level - (0.4 + (-n.edge * 0.2).min(9.0)));
        }
        // Banks: just above the water at its edge, rising to the land's own
        // height, no steeper than 1 in 3.
        let above = ground - n.bank;
        let cut = if above > 0.0 {
            let reach = self.rise.max(3.0 * above).min(FAR_BANK);
            ground.min(
                n.bank + 0.3 + 2.7 * smooth(0.0, 40.0, n.edge) + above * smooth(0.0, reach, n.edge),
            )
        } else {
            ground
        };
        // Where the land lies lower than the water, a levee holds the river.
        let levee = n.level + 0.3 + 1.2 * smooth(0.0, 8.0, n.edge);
        cut + (levee - cut).max(0.0) * (1.0 - smooth(30.0, 60.0, n.edge))
    }

    /// The level the water stands at around `p`: a river's pool where one
    /// lies here, the sea's level (0) everywhere else.
    pub fn water_level(&self, p: DVec2) -> f64 {
        match self.near(p) {
            Some(n) if n.edge < 4.0 => n.level,
            _ => 0.0,
        }
    }

    /// The springs at the heads of the rivers that rise above the sea, inside
    /// the box `lo` to `hi`.
    pub fn springs(&self, lo: DVec2, hi: DVec2) -> Vec<Spring> {
        // What runs through a notch with the water standing SILL_DEPTH in it.
        let flow = 1.7 * NOTCH * SILL_DEPTH * SILL_DEPTH.sqrt();
        self.rivers
            .iter()
            .zip(&self.levels)
            .filter(|(_, l)| l[0] > 0.0)
            .map(|(r, l)| Spring {
                at: r.pts[0] + (r.pts[1] - r.pts[0]).normalize_or_zero() * 4.0,
                level: l[0],
                flow,
            })
            .filter(|s| s.at.cmpge(lo).all() && s.at.cmple(hi).all())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planet::{choose_home, PlanetMap, PlanetPatch};
    use crate::urban::{params::TownParams, world::TownWorld};
    use hw_core::Seed;

    fn town(seed: u64) -> TownWorld {
        let seed = Seed(seed);
        let map = PlanetMap::build(seed, 96);
        let patch = std::sync::Arc::new(PlanetPatch::new(&map, choose_home(&map)));
        let mut params = TownParams::on_planet(seed, 20_000.0, &patch);
        params.rivers = params.rivers.max(1);
        TownWorld::generate_on_or_stop(seed, params, None, &hw_core::Cancel::new())
            .expect("not cancelled")
    }

    #[test]
    fn a_river_carries_on_past_the_town_in_water_all_the_way() {
        let town = town(1);
        let outer = town.outer_rivers().expect("on a planet");
        assert!(!outer.rivers.is_empty());
        for (r, levels) in outer.rivers.iter().zip(&outer.levels) {
            // Under water along the whole centre line, town and country alike:
            // no dam where the town's land ends, no dry stretch in a pool.
            for (q, level) in r.pts.iter().zip(levels) {
                let g = town.ground(*q);
                assert!(g < *level, "dry river bed ({g} m, water at {level}) at {q}");
                assert_eq!(town.water_level(*q), *level);
            }
            // And dry land not far to the side of it along most of its way: a
            // valley, not an arm of the sea. (Not every point: the country may
            // lie at sea level where the river leaves the town.)
            let banked = r
                .pts
                .windows(2)
                .zip(levels)
                .filter(|(w, level)| {
                    let side = (w[1] - w[0]).normalize().perp() * 900.0;
                    town.ground(w[0] + side) > **level || town.ground(w[0] - side) > **level
                })
                .count();
            assert!(
                banked * 2 > r.pts.len(),
                "banks at {banked} of {} points",
                r.pts.len()
            );
        }
    }

    #[test]
    fn a_river_climbs_in_pools_that_hold_their_water() {
        let mut climbed = false;
        for seed in [1, 2, 3] {
            let town = town(seed);
            let outer = town.outer_rivers().expect("on a planet");
            for (r, levels) in outer.rivers.iter().zip(&outer.levels) {
                // Downstream the water only ever steps down, to the sea's
                // level in town, and no fall is taller than the tallest.
                for l in levels.windows(2) {
                    assert!(l[0] >= l[1], "the river runs uphill: {l:?}");
                    assert!(l[0] - l[1] <= TALLEST_FALL);
                }
                assert_eq!(*levels.last().unwrap(), 0.0);
                climbed |= levels[0] > 5.0;
                for (k, w) in r.pts.windows(2).enumerate() {
                    let (a, b) = (w[0], w[1]);
                    let along = (b - a).normalize();
                    let across = along.perp();
                    // The pool is held on both sides: walking away from the
                    // water at a quarter of the stretch (clear of any fall),
                    // the ground comes up above the water before it is far
                    // off, so nothing spills over the land.
                    let q = a + (b - a) * 0.25;
                    let level = levels[k];
                    if level <= 0.0 {
                        continue;
                    }
                    let shore = (r.widths[k] + (r.widths[k + 1] - r.widths[k]) * 0.25) * 0.5;
                    for side in [across, -across] {
                        // A bank stands right beside the water. (Where a
                        // lake narrows to the river again its round end
                        // reaches up to 60 m wider than the width here.)
                        let bank = (0..7)
                            .map(|i| town.ground(q + side * (shore + 15.0 + 10.0 * i as f64)))
                            .fold(f64::MIN, f64::max);
                        assert!(
                            bank > level + 1.0,
                            "seed {seed}: no bank by the pool at {q}: {bank} m, water at {level}"
                        );
                        let held = (1..400)
                            .map(|i| q + side * i as f64 * 2.0)
                            .any(|x| town.ground(x) > level + 0.25);
                        assert!(
                            held,
                            "seed {seed}: the pool at {q} (level {level}) runs out over its bank"
                        );
                    }
                    // A fall: the water runs through the notch in the sill,
                    // and the sill stands above the pool beside the notch.
                    if levels[k] > levels[k + 1] {
                        // The lip, in midstream: where the water's level steps.
                        let len = a.distance(b);
                        let lip = (0..(len as usize))
                            .map(|i| a + along * i as f64)
                            .find(|q| town.water_level(*q) < levels[k])
                            .expect("the river falls on this stretch");
                        let on_sill = lip - along * 3.0;
                        assert!(town.ground(on_sill) < levels[k], "the notch is open");
                        assert!(
                            town.ground(on_sill) > levels[k] - 1.0,
                            "the notch is a sill"
                        );
                        let below = lip + along * 3.0;
                        assert_eq!(town.water_level(below), levels[k + 1]);
                        assert!(town.ground(below) < levels[k + 1], "a pool under the fall");
                    }
                }
                // A spring feeds a river that rises above the sea.
                let all = DVec2::splat(1e9);
                let springs = outer.springs(-all, all);
                assert_eq!(springs.is_empty(), outer.levels.iter().all(|l| l[0] <= 0.0));
                for s in &springs {
                    assert!(town.ground(s.at) < s.level, "the spring rises in its pool");
                }
            }
        }
        assert!(climbed, "none of the rivers climbed above the sea");
    }
}
