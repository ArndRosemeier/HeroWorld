//! The planet on a global grid: heights, climate, rivers, lakes and biomes.
//!
//! [`Globe`] answers "how high is the ground here" for any point; hydrology
//! and climate need the whole planet at once (rain depends on what lies
//! upwind, a river on everything above it), so they are solved once on a
//! cube-sphere grid of about 20 km cells, on all cores, and then looked up.
//! Rivers are refined from this skeleton locally, in later layers.

use super::biome::{classify, Biome};
use super::globe::Globe;
use super::grid::{par_map, Cell, Grid};
use glam::{DVec3, Vec3};
use hw_core::{det, Noise3, Seed};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Flow that makes a river on the map (m³/s).
pub const RIVER_FLOW: f32 = 60.0;
/// A basin filled this deep (m) is a lake.
const LAKE_DEPTH: f64 = 6.0;
const NO_CELL: Cell = Cell::MAX;
/// Share of rain that runs off into rivers (the rest soaks away or evaporates).
const RUNOFF: f64 = 0.35;
/// Moisture units to mm of rain a year.
const RAIN_SCALE: f32 = 42_000.0;
/// Share of rain that goes back into the air over land.
const RECYCLE: f32 = 0.85;
const SECONDS_PER_YEAR: f64 = 31_557_600.0;

/// A landmass.
#[derive(Clone, Debug)]
pub struct Landmass {
    /// Area (km²).
    pub area_km2: f64,
    /// A cell in it.
    pub cell: Cell,
    pub centre: DVec3,
}

/// What the map knows about one place.
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub cell: Cell,
    pub temp: f32,
    pub rain: f32,
    pub biome: Biome,
    /// River flow (m³/s) in this cell; 0 when there is none.
    pub flow: f32,
}

/// The planet solved on a grid.
pub struct PlanetMap {
    pub globe: Globe,
    pub grid: Grid,
    pub centers: Vec<Vec3>,
    /// Height above sea level (m), 20 km detail.
    pub elev: Vec<f32>,
    /// Mean temperature (°C).
    pub temp: Vec<f32>,
    /// Rain (mm a year).
    pub rain: Vec<f32>,
    /// The cell this one drains to; [`NO_CELL`] for the sea.
    pub flow_to: Vec<Cell>,
    /// Water passing through (m³/s).
    pub flow: Vec<f32>,
    /// Depth of the lake in a basin cell (m), 0 if dry.
    pub lake: Vec<f32>,
    pub biome: Vec<Biome>,
}

impl PlanetMap {
    /// Solve the planet of `seed` on a grid of `n x n` cells per face
    /// (512 gives cells of about 20 km).
    pub fn build(seed: Seed, n: usize) -> PlanetMap {
        let globe = Globe::new(seed);
        let grid = Grid::new(n);
        let centers = grid.centers();
        let finest = grid.cell_edge(globe.radius()) * 0.5;
        let elev: Vec<f32> = par_map(grid.cells(), |c| {
            globe.elevation(centers[c].as_dvec3(), finest) as f32
        });
        let temp = temperatures(seed, &centers, &elev);
        let rain = rainfall(&grid, &centers, &elev, &temp);
        let mut map = PlanetMap {
            globe,
            grid,
            centers,
            elev,
            temp,
            rain,
            flow_to: Vec::new(),
            flow: Vec::new(),
            lake: Vec::new(),
            biome: Vec::new(),
        };
        map.solve_rivers();
        map.biome = par_map(map.grid.cells(), |c| map.biome_of(c));
        map
    }

    fn is_sea(&self, c: usize) -> bool {
        self.elev[c] <= 0.0
    }

    fn biome_of(&self, c: usize) -> Biome {
        if self.is_sea(c) {
            return Biome::Ocean;
        }
        if self.lake[c] > 0.0 {
            return Biome::Lake;
        }
        classify(
            self.temp[c] as f64,
            self.rain[c] as f64,
            self.elev[c] as f64,
        )
    }

    /// Fill basins, find where every cell drains, add up the rain.
    fn solve_rivers(&mut self) {
        let cells = self.grid.cells();
        let neigh: Vec<[Cell; 8]> = par_map(cells, |c| self.grid.neighbours(c as Cell));
        let key = |h: f64| {
            let b = h.to_bits();
            if b >> 63 == 1 {
                !b
            } else {
                b | (1 << 63)
            }
        };
        let mut filled: Vec<f64> = self.elev.iter().map(|h| *h as f64).collect();
        let mut closed = vec![false; cells];
        let mut parent = vec![NO_CELL; cells];
        let mut heap: BinaryHeap<Reverse<(u64, Cell)>> = BinaryHeap::new();
        for c in 0..cells {
            if self.is_sea(c) {
                closed[c] = true;
                if neigh[c].iter().any(|n| !self.is_sea(*n as usize)) {
                    heap.push(Reverse((key(filled[c]), c as Cell)));
                }
            }
        }
        let mut order = Vec::with_capacity(cells);
        while let Some(Reverse((_, c))) = heap.pop() {
            let ci = c as usize;
            if !self.is_sea(ci) {
                order.push(c);
            }
            for &nb in &neigh[ci] {
                let ni = nb as usize;
                if closed[ni] {
                    continue;
                }
                closed[ni] = true;
                parent[ni] = c;
                // Drain downhill at least a hair, so flats and pits flow out.
                filled[ni] = filled[ni].max(filled[ci] + 0.001);
                heap.push(Reverse((key(filled[ni]), nb)));
            }
        }
        // Rain that runs off, in m³/s.
        let area = self.grid.cell_area(self.globe.radius());
        let mut flow: Vec<f64> = (0..cells)
            .map(|c| {
                if self.is_sea(c) {
                    0.0
                } else {
                    self.rain[c] as f64 / 1000.0 * area * RUNOFF / SECONDS_PER_YEAR
                }
            })
            .collect();
        for &c in order.iter().rev() {
            let p = parent[c as usize];
            if p != NO_CELL {
                flow[p as usize] += flow[c as usize];
            }
        }
        self.lake = (0..cells)
            .map(|c| {
                let depth = filled[c] - self.elev[c] as f64;
                if !self.is_sea(c) && depth > LAKE_DEPTH && self.rain[c] > 300.0 {
                    depth as f32
                } else {
                    0.0
                }
            })
            .collect();
        self.flow = flow.into_iter().map(|f| f as f32).collect();
        self.flow_to = parent;
    }

    /// What the map says at the direction `u`.
    pub fn sample(&self, u: DVec3) -> Sample {
        let cell = self.grid.cell_of(u);
        let c = cell as usize;
        Sample {
            cell,
            temp: self.temp[c],
            rain: self.rain[c],
            biome: self.biome[c],
            flow: if self.flow[c] >= RIVER_FLOW && !self.is_sea(c) {
                self.flow[c]
            } else {
                0.0
            },
        }
    }

    pub fn is_river(&self, c: Cell) -> bool {
        self.flow[c as usize] >= RIVER_FLOW && !self.is_sea(c as usize)
    }

    /// The landmasses, biggest first (islands of one cell are left out).
    pub fn landmasses(&self) -> Vec<Landmass> {
        let cells = self.grid.cells();
        let area = self.grid.cell_area(self.globe.radius()) / 1e6;
        let mut seen = vec![false; cells];
        let mut out = Vec::new();
        for start in 0..cells {
            if seen[start] || self.is_sea(start) {
                continue;
            }
            seen[start] = true;
            let mut stack = vec![start as Cell];
            let (mut count, mut sum) = (0usize, DVec3::ZERO);
            while let Some(c) = stack.pop() {
                count += 1;
                sum += self.centers[c as usize].as_dvec3();
                for nb in self.grid.neighbours(c) {
                    let ni = nb as usize;
                    if !seen[ni] && !self.is_sea(ni) {
                        seen[ni] = true;
                        stack.push(nb);
                    }
                }
            }
            if count > 1 {
                out.push(Landmass {
                    area_km2: count as f64 * area,
                    cell: start as Cell,
                    centre: sum.normalize_or(DVec3::Y),
                });
            }
        }
        out.sort_by(|a, b| b.area_km2.total_cmp(&a.area_km2));
        out
    }

    /// Share of the surface that is land.
    pub fn land_share(&self) -> f64 {
        (0..self.grid.cells()).filter(|c| !self.is_sea(*c)).count() as f64
            / self.grid.cells() as f64
    }
}

/// Mean temperature (°C): warm at the equator, cold at the poles and up high.
fn temperatures(seed: Seed, centers: &[Vec3], elev: &[f32]) -> Vec<f32> {
    let noise = Noise3::new(seed.child("temperature"));
    par_map(centers.len(), |c| {
        let p = centers[c].as_dvec3();
        let sin_lat = p.y;
        let lapse = 6.5 * (elev[c].max(0.0) as f64 / 1000.0);
        let wobble = 2.5 * noise.fbm(p.x * 3.0, p.y * 3.0, p.z * 3.0, 3);
        let s2 = sin_lat * sin_lat;
        (28.0 - 34.0 * s2 - 14.0 * s2 * s2 * s2 - lapse + wobble) as f32
    })
}

/// Rain (mm a year) from moist air carried by the prevailing winds: trade
/// winds near the equator, westerlies in the middle latitudes, polar easterlies.
/// Air picks up water over the sea and drops it over rising ground, so
/// windward coasts and mountains are wet and the far side of them dry.
fn rainfall(grid: &Grid, centers: &[Vec3], elev: &[f32], temp: &[f32]) -> Vec<f32> {
    let cells = grid.cells();
    // For every cell: its (up to) three upwind neighbours with weights.
    let upwind: Vec<[(Cell, f32); 3]> = par_map(cells, |c| {
        let p = centers[c].as_dvec3();
        let lat = det::asin(p.y.clamp(-1.0, 1.0));
        let u = -det::cos(3.0 * lat);
        let v = -0.45 * p.y.signum() * det::cos(6.0 * lat);
        let mut east = DVec3::Y.cross(p);
        if east.length_squared() < 1e-12 {
            east = DVec3::X;
        }
        let east = east.normalize();
        let north = p.cross(east);
        let wind = (east * u + north * v).normalize_or(east);
        let mut scored: Vec<(f64, Cell)> = grid
            .neighbours(c as Cell)
            .iter()
            .map(|&nb| {
                let d = (centers[nb as usize].as_dvec3() - p).normalize_or(east);
                (-wind.dot(d), nb)
            })
            .filter(|(s, _)| *s > 0.05)
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut out = [(0, 0.0f32); 3];
        let total: f64 = scored.iter().take(3).map(|s| s.0).sum();
        for (k, (s, nb)) in scored.iter().take(3).enumerate() {
            out[k] = (*nb, (s / total) as f32);
        }
        out
    });
    // Rain factor by latitude: wet at the equator and near 55 degrees, dry near 27.
    let lat_factor: Vec<f32> = par_map(cells, |c| {
        let lat = det::asin((centers[c].y as f64).clamp(-1.0, 1.0))
            .abs()
            .to_degrees();
        let bump = |centre: f64, width: f64| det::exp(-((lat - centre) / width).powi(2));
        (0.5 + 1.1 * bump(0.0, 12.0) + 0.5 * bump(55.0, 12.0) - 0.35 * bump(27.0, 9.0)) as f32
    });
    let evap: Vec<f32> = (0..cells)
        .map(|c| (0.35 + 0.03 * temp[c]).clamp(0.08, 1.1))
        .collect();
    let sea: Vec<bool> = elev.iter().map(|h| *h <= 0.0).collect();
    let mut moist: Vec<f32> = (0..cells)
        .map(|c| if sea[c] { evap[c] } else { 0.2 })
        .collect();
    let mut rain = vec![0.0f32; cells];
    for _ in 0..110 {
        let step: Vec<(f32, f32)> = par_map(cells, |c| {
            let ups = &upwind[c];
            let (mut m, mut h_up, mut w_sum) = (0.0f32, 0.0f32, 0.0f32);
            for &(nb, w) in ups {
                if w > 0.0 {
                    m += moist[nb as usize] * w;
                    h_up += elev[nb as usize].max(0.0) * w;
                    w_sum += w;
                }
            }
            if w_sum == 0.0 {
                m = moist[c];
                h_up = elev[c].max(0.0);
            }
            let uplift = ((elev[c].max(0.0) - h_up) / 1000.0).clamp(0.0, 1.0);
            let frac = (0.035 * lat_factor[c] + 0.25 * uplift).min(0.5);
            let cold = 0.7 + 0.3 * ((temp[c] + 20.0) / 45.0).clamp(0.0, 1.0);
            let r = m * frac * cold;
            let kept = if sea[c] {
                evap[c]
            } else {
                (m - r + RECYCLE * r).max(0.0)
            };
            (kept, r)
        });
        for (c, (m, r)) in step.into_iter().enumerate() {
            moist[c] = m;
            rain[c] = r * RAIN_SCALE;
        }
    }
    rain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_planet_is_deterministic_with_sane_climate_and_rivers() {
        let a = PlanetMap::build(Seed(11), 48);
        let b = PlanetMap::build(Seed(11), 48);
        assert_eq!(a.elev, b.elev);
        assert_eq!(a.rain, b.rain);
        assert_eq!(a.flow_to, b.flow_to);
        assert_eq!(a.flow, b.flow);
        assert!((a.land_share() - 0.30).abs() < 0.08, "{}", a.land_share());
        assert!(a.landmasses().len() >= 2);
        // Warm at the equator, cold at the poles.
        let eq = a.sample(DVec3::new(1.0, 0.0, 0.0)).temp;
        let pole = a.sample(DVec3::Y).temp;
        assert!(eq > pole + 25.0, "{eq} vs {pole}");
        // Every river follows the water downhill to the sea or to a lake.
        for c in 0..a.grid.cells() {
            if a.is_sea(c) || a.flow_to[c] == NO_CELL {
                continue;
            }
            let mut at = c;
            for _ in 0..a.grid.cells() {
                let next = a.flow_to[at];
                if next == NO_CELL {
                    break;
                }
                at = next as usize;
            }
            assert!(a.is_sea(at), "cell {c} drains nowhere");
        }
        // Rain differs (not one value), and rivers exist.
        let wet = a.rain.iter().cloned().fold(0.0, f32::max);
        let dry = a.rain.iter().cloned().fold(f32::MAX, f32::min);
        assert!(wet > 4.0 * dry.max(1.0), "{dry}..{wet}");
        assert!((0..a.grid.cells()).any(|c| a.is_river(c as Cell)));
    }
}
