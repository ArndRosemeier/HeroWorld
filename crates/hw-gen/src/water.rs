//! A city's water: one or two rivers and, for most cities, a coast.
//!
//! Water is planned before the streets, and the city is laid out around it
//! (see [`crate::city`]). Rivers and the coast are polylines of straight
//! reaches. A river's water is a band `width` wide around its line; the sea
//! lies on one side of the coastline.
//!
//! For now the water comes from the city seed. Once the planet layer exists,
//! cities will sit where its rivers meet its coasts, and this will be filled
//! from there instead.

use glam::DVec2;
use hw_core::{det, Seed};

/// Height of the water surface below the streets (m).
pub const WATER_LEVEL: f64 = -3.5;

/// Paved promenade along a river, between the quay wall and the blocks (m):
/// a towpath along a brook, a broad quay along a big river.
pub fn quay_width(river_width: f64) -> f64 {
    (river_width * 0.12).clamp(3.0, 9.0)
}

/// Beach and promenade along the coast, between the sea and the blocks (m).
pub const SHORE_WIDTH: f64 = 30.0;

/// Promenade behind the beach (the rest of [`SHORE_WIDTH`] is sand).
pub const PROMENADE_WIDTH: f64 = 8.0;

#[derive(Clone, Debug, PartialEq)]
pub struct River {
    /// The centre line, from upstream to downstream.
    pub pts: Vec<DVec2>,
    /// Width of the water (m).
    pub width: f64,
}

/// The coastline. Walking along `pts`, the sea is on the right (negative
/// side of `(b - a).perp_dot(p - a)`), the city on the left.
#[derive(Clone, Debug, PartialEq)]
pub struct Coast {
    pub pts: Vec<DVec2>,
    /// Unit vector pointing out to sea.
    pub seaward: DVec2,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CityWater {
    pub rivers: Vec<River>,
    pub coast: Option<Coast>,
}

/// Distance from `p` to segment `ab`.
pub fn segment_distance(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

impl Coast {
    /// Signed distance to the coastline: positive on land, negative at sea.
    pub fn land_distance(&self, p: DVec2) -> f64 {
        let mut best = (f64::MAX, 1.0);
        for w in self.pts.windows(2) {
            let d = segment_distance(p, w[0], w[1]);
            if d < best.0 {
                let side = (w[1] - w[0]).perp_dot(p - w[0]);
                best = (d, if side >= 0.0 { 1.0 } else { -1.0 });
            }
        }
        best.0 * best.1
    }
}

impl River {
    pub fn distance(&self, p: DVec2) -> f64 {
        self.pts
            .windows(2)
            .map(|w| segment_distance(p, w[0], w[1]))
            .fold(f64::MAX, f64::min)
    }
}

impl CityWater {
    /// Plans the water of a city of `radius` around the origin. Lines run
    /// on to `reach` from the centre, past the edge of the built-up area.
    pub fn generate(seed: Seed, radius: f64, reach: f64) -> CityWater {
        let seed = seed.child("water");
        let mut rng = seed.rng();
        let coastal = rng.chance(0.6);
        let sea_angle = rng.range(0.0, std::f64::consts::TAU);
        let seaward = det::dir2(sea_angle);
        let coast = coastal.then(|| {
            let along = seaward.perp();
            let dist = radius * rng.range(0.55, 0.85);
            // A gentle wave plus one bay or headland.
            let (k1, p1) = (rng.range(1.0, 2.0), rng.range(0.0, std::f64::consts::TAU));
            let amp = rng.range(0.03, 0.085) * radius;
            let bay_at = rng.range(-0.6, 0.6) * radius;
            let bay = rng.range(-0.12, 0.25) * radius;
            let step = (radius * 0.17).clamp(40.0, 400.0);
            let n = (reach / step).ceil() as i64;
            let pts: Vec<DVec2> = (-n..=n)
                .map(|i| {
                    let t = i as f64 * step;
                    let wave = amp * det::sin(k1 * t / radius + p1);
                    let x = (t - bay_at) / (radius * 0.35);
                    let off = dist + wave + bay * det::exp(-x * x);
                    along * t + seaward * off
                })
                .collect();
            // Walking along +along, seaward must be on the right.
            let mut c = Coast { pts, seaward };
            if c.land_distance(DVec2::ZERO) < 0.0 {
                c.pts.reverse();
            }
            c
        });
        let mut rivers = Vec::new();
        // The main river passes close to the centre (the city grew on it)
        // and flows out to sea when there is one.
        let flow = if coastal {
            det::dir2(sea_angle + rng.range(-0.5, 0.5))
        } else {
            det::dir2(rng.range(0.0, std::f64::consts::TAU))
        };
        let through = DVec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0)) * radius * 0.15;
        let down = meander(&mut rng, through, flow, reach, radius);
        let up = meander(&mut rng, through, -flow, reach, radius);
        let mut pts: Vec<DVec2> = up.into_iter().rev().collect();
        pts.extend(down.into_iter().skip(1));
        // A brook through a hamlet, a broad river through a metropolis.
        let width = (radius * 0.06).clamp(8.0, 220.0);
        rivers.push(River {
            pts,
            width: width * rng.range(0.8, 1.25),
        });
        if rng.chance(0.35) {
            // A tributary joins from upstream, at an angle to the main river.
            let main = &rivers[0];
            let mid = main.pts.len() / 2;
            let k =
                (mid as i64 + rng.range_int(-2, 1)).clamp(1, main.pts.len() as i64 - 2) as usize;
            let (a, b) = (main.pts[k], main.pts[k + 1]);
            let join = a.lerp(b, rng.range(0.3, 0.7));
            let dir = (b - a).normalize();
            let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
            let mut out = (-dir * 0.6 + dir.perp() * side * 0.8).normalize();
            if let Some(c) = &coast {
                // Away from the sea.
                if out.dot(c.seaward) > 0.0 {
                    out = (-dir * 0.6 - dir.perp() * side * 0.8).normalize();
                }
            }
            let arm = meander(&mut rng, join, out, reach, radius);
            rivers.push(River {
                pts: arm.into_iter().rev().collect(),
                width: width * rng.range(0.45, 0.7),
            });
        }
        CityWater { rivers, coast }
    }

    /// Is `p` open water (sea, or a river)?
    pub fn is_water(&self, p: DVec2) -> bool {
        self.coast
            .as_ref()
            .is_some_and(|c| c.land_distance(p) < 0.0)
            || self.rivers.iter().any(|r| r.distance(p) < r.width * 0.5)
    }
}

/// A meandering line from `start` heading `dir` until `reach` from the
/// centre, in reaches of about a quarter of the city radius.
fn meander(
    rng: &mut hw_core::Rng,
    start: DVec2,
    dir: DVec2,
    reach: f64,
    radius: f64,
) -> Vec<DVec2> {
    let mut pts = vec![start];
    let mut heading = det::atan2(dir.y, dir.x);
    let base = heading;
    let mut p = start;
    while p.length() < reach && pts.len() < 400 {
        let turn = rng.range(-0.4, 0.4) + (base - heading) * 0.5;
        heading += turn;
        p += det::dir2(heading) * rng.range(0.24, 0.32) * radius.clamp(250.0, 2200.0);
        pts.push(p);
    }
    pts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn every_city_has_a_river_and_most_a_coast() {
        let mut coastal = 0;
        let mut two = 0;
        for s in 0..60 {
            let w = CityWater::generate(Seed(s), 1300.0, 2500.0);
            assert!(!w.rivers.is_empty());
            coastal += w.coast.is_some() as u32;
            two += (w.rivers.len() > 1) as u32;
            // The centre is dry land, and the main river passes near it.
            assert!(!w.is_water(DVec2::ZERO) || w.rivers[0].distance(DVec2::ZERO) < 260.0);
            if let Some(c) = &w.coast {
                assert!(c.land_distance(DVec2::ZERO) > 400.0);
            }
            assert!(w.rivers[0].distance(DVec2::ZERO) < 300.0);
        }
        assert!((20..50).contains(&coastal), "{coastal} coastal of 60");
        assert!((8..35).contains(&two), "{two} with two rivers");
    }

    #[test]
    fn same_seed_same_water() {
        assert_eq!(
            CityWater::generate(Seed(3), 1300.0, 2500.0),
            CityWater::generate(Seed(3), 1300.0, 2500.0)
        );
    }
}
