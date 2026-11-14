//! The home region: where on the planet the first city stands, and the
//! ground around it.
//!
//! [`choose_home`] looks over the planet map for a temperate river mouth
//! (the start region is meant to be ordinary, with the Aether at its
//! lightest). [`PlanetPatch`] then gives the planet's ground in local metres
//! (x east, z south, as the town uses) with the curve of the planet in it,
//! and says what the place is like: its coast, its river, its relief.

use super::globe::Globe;
use super::map::PlanetMap;
use crate::urban::params::Site;
use glam::{DVec2, DVec3};
use hw_core::{det, LatLon, LocalFrame, Planet, PlanetPos};

/// Detail of the ground around the home (m): finer than a tile's vertices.
const GROUND_DETAIL: f64 = 40.0;

/// A temperate river mouth on low ground with weak Aether, as a unit vector.
pub fn choose_home(map: &PlanetMap) -> DVec3 {
    let cells = map.grid.cells();
    let mut best: Option<(f64, usize)> = None;
    for strict in [true, false] {
        for c in 0..cells {
            let (h, t, r) = (map.elev[c], map.temp[c], map.rain[c]);
            let y = map.centers[c].y.abs();
            if h <= 2.0 || h > 150.0 || y > 0.8 {
                continue;
            }
            if strict && !((8.0..=17.0).contains(&t) && (600.0..=1400.0).contains(&r)) {
                continue;
            }
            let flow = map.flow[c];
            if flow < 150.0 || !map.is_river(c as u32) {
                continue;
            }
            // Next to the sea, with land behind.
            let ns = map.grid.neighbours(c as u32);
            if !ns.iter().any(|n| map.elev[*n as usize] <= 0.0)
                || !ns.iter().any(|n| map.elev[*n as usize] > 2.0)
            {
                continue;
            }
            let aether = map.globe.aether(map.centers[c].as_dvec3());
            if strict && aether > 0.45 {
                continue;
            }
            // Mountains in sight make the place: the highest ground on rings
            // 15 and 30 km out.
            let up = map.centers[c].as_dvec3();
            let east = DVec3::Y.cross(up).normalize_or(DVec3::X);
            let north = up.cross(east);
            let peak = (0..32)
                .map(|k| {
                    let a = (k / 2) as f64 / 16.0 * std::f64::consts::TAU;
                    let far = if k % 2 == 0 { 15_000.0 } else { 30_000.0 };
                    let (s, co) = det::sin_cos(a);
                    let dir =
                        (up + (east * co + north * s) * (far / map.globe.radius())).normalize();
                    map.globe.elevation(dir, 1000.0)
                })
                .fold(0.0, f64::max);
            let score = (flow as f64).ln() - 4.0 * aether - (t as f64 - 12.0).abs() * 0.1
                + peak.min(3000.0) / 400.0
                - if peak < 1000.0 { 3.0 } else { 0.0 };
            if best.is_none_or(|(s, bc)| score > s || (score == s && c < bc)) {
                best = Some((score, c));
            }
        }
        if best.is_some() {
            break;
        }
    }
    // A planet without any: the lowest land of the temperate belt.
    let c = best.map_or_else(
        || {
            (0..cells)
                .filter(|c| map.elev[*c] > 2.0 && map.centers[*c].y.abs() < 0.6)
                .min_by(|a, b| map.elev[*a].total_cmp(&map.elev[*b]))
                .unwrap_or(0)
        },
        |b| b.1,
    );
    map.centers[c].as_dvec3()
}

/// What the home place is like, from the planet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PatchSite {
    pub site: Site,
    /// 0 flat .. 1 mountainous.
    pub relief: f64,
    pub rivers: u8,
    pub river_width: f64,
    /// Direction out to sea (radians, 0 = east, π/2 = south), if coastal.
    pub seaward: Option<f64>,
    /// How far the sea lies in that direction (m).
    pub coast_distance: Option<f64>,
    /// Direction the river flows (radians, same convention), if there is one.
    pub flow: Option<f64>,
}

/// The planet's ground around one place, in the place's local metres.
#[derive(Clone, Debug)]
pub struct PlanetPatch {
    globe: Globe,
    frame: LocalFrame,
    planet: Planet,
    pub site: PatchSite,
    /// Sea-level temperature (°C) and rain (mm a year) on a grid of
    /// [`CLIMATE_N`]² points [`CLIMATE_STEP`] apart, centred on the place.
    climate: Vec<(f32, f32)>,
}

/// Spacing of the patch's climate grid (m) and its points per side.
const CLIMATE_STEP: f64 = 16_000.0;
const CLIMATE_N: usize = 33;
/// Air cools by this much per metre of height (°C).
const LAPSE: f64 = 6.5 / 1000.0;

/// What covers the ground at a point of the patch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cover {
    /// 1 grass .. 0 bare ground (dry country, or scree above the meadows).
    pub green: f64,
    /// Local height above which snow lies (m, in the patch's heights).
    pub snow_line: f64,
    /// How much of the ground trees cover, 0 to 1.
    pub trees: f64,
    /// Mean temperature here (°C).
    pub temp: f64,
}

impl PartialEq for PlanetPatch {
    /// Two patches of one planet are the same when they stand in the same place.
    fn eq(&self, other: &Self) -> bool {
        self.frame == other.frame && self.planet == other.planet && self.site == other.site
    }
}

impl PlanetPatch {
    /// The patch at `at` (a unit vector); `map` tells what the place is like.
    pub fn new(map: &PlanetMap, at: DVec3) -> Self {
        let planet = Planet {
            radius: map.globe.radius(),
        };
        let frame_at = |u: DVec3| LocalFrame::at(PlanetPos::at(&planet, LatLon::from_dir(u), 0.0));
        let mut patch = PlanetPatch {
            globe: map.globe.clone(),
            frame: frame_at(at),
            planet,
            site: PatchSite {
                site: Site::Crossroads,
                relief: 0.3,
                rivers: 0,
                river_width: 20.0,
                seaward: None,
                coast_distance: None,
                flow: None,
            },
            climate: Vec::new(),
        };
        // Stand where the real coast is a few hundred metres off, so the
        // town's sea is the planet's sea.
        let local = patch.coast_spot();
        let moved = patch
            .frame
            .to_planet(DVec3::new(local.x, 0.0, local.y))
            .up();
        patch.frame = frame_at(moved);
        patch.site = patch.describe(map, moved);
        patch.climate = patch.read_climate(map);
        patch
    }

    /// The map's climate around the place, taken down to sea level and
    /// averaged over each cell and its neighbours, so it changes smoothly.
    fn read_climate(&self, map: &PlanetMap) -> Vec<(f32, f32)> {
        let half = (CLIMATE_N / 2) as f64;
        let mut out = Vec::with_capacity(CLIMATE_N * CLIMATE_N);
        for j in 0..CLIMATE_N {
            for i in 0..CLIMATE_N {
                let p = DVec2::new(i as f64 - half, j as f64 - half) * CLIMATE_STEP;
                let u = self.frame.to_planet(DVec3::new(p.x, 0.0, p.y)).up();
                let cell = map.grid.cell_of(u);
                let (mut t, mut r, mut n) = (0.0, 0.0, 0.0);
                for c in std::iter::once(cell).chain(map.grid.neighbours(cell)) {
                    let c = c as usize;
                    t += map.temp[c] as f64 + LAPSE * (map.elev[c] as f64).max(0.0);
                    r += map.rain[c] as f64;
                    n += 1.0;
                }
                out.push(((t / n) as f32, (r / n) as f32));
            }
        }
        out
    }

    /// Sea-level temperature (°C) and rain (mm a year) at local `p`.
    pub fn climate(&self, p: DVec2) -> (f64, f64) {
        let half = (CLIMATE_N / 2) as f64;
        let max = (CLIMATE_N - 1) as f64;
        let g = (p / CLIMATE_STEP + DVec2::splat(half)).clamp(DVec2::ZERO, DVec2::splat(max));
        let (i, j) = (
            (g.x as usize).min(CLIMATE_N - 2),
            (g.y as usize).min(CLIMATE_N - 2),
        );
        let (fx, fy) = (g.x - i as f64, g.y - j as f64);
        let at = |i: usize, j: usize| {
            let (t, r) = self.climate[j * CLIMATE_N + i];
            DVec2::new(t as f64, r as f64)
        };
        let v = at(i, j)
            .lerp(at(i + 1, j), fx)
            .lerp(at(i, j + 1).lerp(at(i + 1, j + 1), fx), fy);
        (v.x, v.y)
    }

    /// What covers the ground at local `p`, where the ground stands at
    /// `height` (as [`Self::ground`] gives it). Grass thins out where it is
    /// dry and towards the snow; trees stop well below it.
    pub fn cover(&self, p: DVec2, height: f64) -> Cover {
        let (sea_temp, rain) = self.climate(p);
        let drop = self.frame.ground_drop(&self.planet, p.length());
        let real = height + drop;
        let temp = sea_temp - LAPSE * real.max(0.0);
        // Snow lies where the year's mean is below -1 °C.
        let snow_real = (sea_temp + 1.0) / LAPSE;
        let step = |a: f64, b: f64, x: f64| {
            let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        let wet = step(180.0, 520.0, rain);
        let below_scree = 1.0 - step(snow_real - 900.0, snow_real - 250.0, real);
        let biome = super::biome::classify(temp, rain, real);
        let below_trees = 1.0 - step(snow_real - 1500.0, snow_real - 1000.0, real);
        Cover {
            green: wet * below_scree,
            snow_line: snow_real - drop,
            trees: biome.tree_cover() * below_trees,
            temp,
        }
    }

    /// Is `p` in the sea proper, looking along `d`: under water here and
    /// still under water 1.5 and 3 km further out (not a pond or a creek)?
    fn open_sea(&self, p: DVec2, d: DVec2) -> bool {
        self.ground(p) < 0.0
            && self.ground(p + d * 1500.0) < -3.0
            && self.ground(p + d * 3000.0) < -3.0
    }

    /// Distance to the sea along the nearest of sixteen rays from `p`
    /// (m), and the direction; `None` if there is no sea within 3 km.
    fn sea_from(&self, p: DVec2) -> Option<(f64, DVec2)> {
        let mut best: Option<(f64, DVec2)> = None;
        for k in 0..16 {
            let d = det::dir2(k as f64 / 16.0 * std::f64::consts::TAU);
            for i in 1..=20 {
                let t = 150.0 * i as f64;
                if self.open_sea(p + d * t, d) {
                    if best.is_none_or(|b| t < b.0) {
                        best = Some((t, d));
                    }
                    break;
                }
            }
        }
        best
    }

    /// Ground height (m above sea level) at local `p`, with the planet's
    /// curve: the ground falls away from the tangent plane by `d² / 2R`.
    pub fn ground(&self, p: DVec2) -> f64 {
        let at = self.frame.to_planet(DVec3::new(p.x, 0.0, p.y));
        let drop = self.frame.ground_drop(&self.planet, p.length());
        self.globe.elevation(at.up(), GROUND_DETAIL) - drop
    }

    /// A spot on low dry land whose coast lies about 700 m away: found by
    /// walking out from the patch's origin until the sea, then back again.
    fn coast_spot(&self) -> DVec2 {
        let mut nearest: Option<(f64, DVec2)> = None;
        for k in 0..32 {
            let d = det::dir2(k as f64 / 32.0 * std::f64::consts::TAU);
            for i in 1..=120 {
                let t = 250.0 * i as f64;
                if self.open_sea(d * t, d) {
                    if nearest.is_none_or(|n| t < n.0) {
                        // Refine the shore along the ray, then step 700 m inland.
                        let (mut lo, mut hi) = (t - 250.0, t);
                        for _ in 0..8 {
                            let mid = 0.5 * (lo + hi);
                            if self.ground(d * mid) < 0.0 {
                                hi = mid;
                            } else {
                                lo = mid;
                            }
                        }
                        nearest = Some((t, d * (hi - 700.0).max(0.0)));
                    }
                    break;
                }
            }
        }
        nearest.map_or(DVec2::ZERO, |n| n.1)
    }

    fn describe(&self, map: &PlanetMap, at: DVec3) -> PatchSite {
        // Which way is the sea?
        let sea = self.sea_from(DVec2::ZERO);
        let seaward = sea.map(|(_, d)| det::atan2(d.y, d.x));
        let coast_distance = sea.map(|(t, _)| t);
        // The river nearest the place: its own cell or a neighbour's.
        let cell = map.grid.cell_of(at);
        let mut c = cell as usize;
        for nb in map.grid.neighbours(cell) {
            if map.elev[nb as usize] > 0.0 && map.flow[nb as usize] > map.flow[c] {
                c = nb as usize;
            }
        }
        let q = map.flow[c] as f64;
        let river = q >= 60.0 && map.elev[c] > 0.0;
        let flow = river
            .then(|| map.flow_to[c])
            .filter(|n| *n != u32::MAX)
            .map(|n| {
                let to = self.frame.to_local(PlanetPos(
                    map.centers[n as usize].as_dvec3() * self.planet.radius,
                ));
                let from = self
                    .frame
                    .to_local(PlanetPos(map.centers[c].as_dvec3() * self.planet.radius));
                det::atan2(to.z - from.z, to.x - from.x)
            });
        // Relief: how much the ground varies within a few km.
        let samples: Vec<f64> = (0..24)
            .map(|k| {
                let a = k as f64 / 24.0 * std::f64::consts::TAU;
                self.ground(det::dir2(a) * 3000.0)
            })
            .collect();
        let mean = samples.iter().sum::<f64>() / samples.len() as f64;
        let sd =
            (samples.iter().map(|h| (h - mean).powi(2)).sum::<f64>() / samples.len() as f64).sqrt();
        let site = match (seaward.is_some(), river) {
            (true, true) => Site::Estuary,
            (true, false) => Site::Coast,
            (false, true) => Site::RiverCrossing,
            _ => Site::Crossroads,
        };
        PatchSite {
            site,
            relief: (sd / 250.0).clamp(0.05, 1.0),
            rivers: river as u8,
            river_width: (2.7 * q.sqrt()).clamp(8.0, 400.0),
            seaward,
            coast_distance,
            flow,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hw_core::Seed;

    #[test]
    fn the_home_is_a_temperate_river_mouth_and_its_ground_is_continuous() {
        let map = PlanetMap::build(Seed(2), 96);
        let home = choose_home(&map);
        let again = choose_home(&map);
        assert_eq!(home, again);
        let c = map.grid.cell_of(home) as usize;
        assert!(map.elev[c] > 0.0 && map.elev[c] <= 150.0, "{}", map.elev[c]);
        assert!(map.flow[c] >= 150.0, "a river mouth: {}", map.flow[c]);
        let patch = PlanetPatch::new(&map, home);
        // Ground is smooth: neighbouring points a metre apart differ little.
        for k in 0..50 {
            let p = DVec2::new(k as f64 * 137.0 - 3000.0, k as f64 * 91.0 - 2000.0);
            let (a, b) = (patch.ground(p), patch.ground(p + DVec2::new(1.0, 0.0)));
            assert!((a - b).abs() < 5.0, "{a} vs {b}");
        }
        // The planet curves away: 30 km out is lower than the plane by its drop.
        assert!(patch.site.relief > 0.0);
        // The home is green low country; high up there is scree, then snow,
        // and no trees.
        let low = patch.cover(DVec2::ZERO, 20.0);
        assert!(low.green > 0.9 && low.trees > 0.3, "{low:?}");
        assert!((5.0..=20.0).contains(&low.temp), "{low:?}");
        assert!((1200.0..=4500.0).contains(&low.snow_line), "{low:?}");
        let high = patch.cover(DVec2::ZERO, low.snow_line + 100.0);
        assert!(high.green == 0.0 && high.trees == 0.0, "{high:?}");
        // Climate is smooth across the grid's points.
        for k in 0..40 {
            let p = DVec2::new(k as f64 * 3100.0 - 60_000.0, k as f64 * 1700.0 - 30_000.0);
            let (a, b) = (patch.climate(p), patch.climate(p + DVec2::X * 100.0));
            assert!(
                (a.0 - b.0).abs() < 0.2 && (a.1 - b.1).abs() < 20.0,
                "{a:?} {b:?}"
            );
        }
    }

    #[test]
    fn a_town_on_the_planet_blends_into_the_planet_without_a_wall() {
        use crate::urban::{params::TownParams, world::TownWorld};
        let seed = Seed(2);
        let map = PlanetMap::build(seed, 96);
        let patch = std::sync::Arc::new(PlanetPatch::new(&map, choose_home(&map)));
        let params = TownParams::on_planet(seed, 20_000.0, &patch);
        assert_eq!(params.site, patch.site.site);
        let town = TownWorld::generate_on_or_stop(
            seed,
            params,
            Some(patch.clone()),
            &hw_core::Cancel::new(),
        )
        .expect("not cancelled");
        let half = town.half();
        // Walk out along eight rays, past the edge of the planned land, in
        // 25 m steps: the ground never jumps.
        for k in 0..8 {
            let d = det::dir2(k as f64 / 8.0 * std::f64::consts::TAU);
            let mut last = town.ground(DVec2::ZERO);
            let mut worst: f64 = 0.0;
            let mut t = 25.0;
            while t < half * 2.5 {
                let g = town.ground(d * t);
                worst = worst.max((g - last).abs());
                last = g;
                t += 25.0;
            }
            assert!(worst < 40.0, "ray {k}: a step of {worst} m");
            // Far out it is the planet's ground.
            let far = d * (half * 2.5);
            assert_eq!(town.ground(far), patch.ground(far));
        }
    }
}
