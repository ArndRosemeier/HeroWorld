//! The land under and around a settlement: terrain heights, the sea, lakes
//! and rivers. The town reads it and never contradicts it: growth stops at
//! water and slows on slopes, roads follow valleys, quays line the rivers.
//!
//! Water is planned as geometry first (a coastline, lake shores, river
//! lines), then rasterised; the terrain is shaped around it so it falls to
//! the water. For now everything comes from the town's seed and parameters;
//! later the planet layer will hand over its own coast and rivers here.

use super::params::{Site, TownParams};
use crate::water::segment_distance;
use glam::DVec2;
use hw_core::{det, Noise, Rng, Seed};

/// A square raster centred on the town's founding point.
#[derive(Clone, Debug, PartialEq)]
pub struct Grid<T> {
    /// Corner of cell (0, 0).
    pub origin: DVec2,
    /// Cell size (m).
    pub cell: f64,
    pub w: usize,
    pub h: usize,
    pub data: Vec<T>,
}

impl<T: Clone> Grid<T> {
    pub fn new(half: f64, cell: f64, fill: T) -> Self {
        let n = ((2.0 * half) / cell).ceil() as usize;
        Grid {
            origin: DVec2::splat(-(n as f64) * cell * 0.5),
            cell,
            w: n,
            h: n,
            data: vec![fill; n * n],
        }
    }

    pub fn at(&self, i: usize, j: usize) -> &T {
        &self.data[j * self.w + i]
    }

    pub fn set(&mut self, i: usize, j: usize, v: T) {
        self.data[j * self.w + i] = v;
    }

    /// Centre of cell (i, j).
    pub fn center(&self, i: usize, j: usize) -> DVec2 {
        self.origin + DVec2::new(i as f64 + 0.5, j as f64 + 0.5) * self.cell
    }

    /// The cell holding `p`, if inside.
    pub fn cell_of(&self, p: DVec2) -> Option<(usize, usize)> {
        let q = (p - self.origin) / self.cell;
        (q.x >= 0.0 && q.y >= 0.0 && (q.x as usize) < self.w && (q.y as usize) < self.h)
            .then_some((q.x as usize, q.y as usize))
    }

    /// Half the side of the square (m).
    pub fn half(&self) -> f64 {
        self.w as f64 * self.cell * 0.5
    }
}

impl Grid<f32> {
    /// Bilinear sample at `p` (clamped to the edge).
    pub fn sample(&self, p: DVec2) -> f64 {
        let q = (p - self.origin) / self.cell - DVec2::splat(0.5);
        let x = q.x.clamp(0.0, (self.w - 1) as f64);
        let y = q.y.clamp(0.0, (self.h - 1) as f64);
        let (i, j) = (x.floor() as usize, y.floor() as usize);
        let (i1, j1) = ((i + 1).min(self.w - 1), (j + 1).min(self.h - 1));
        let (tx, ty) = (x - i as f64, y - j as f64);
        let v = |i: usize, j: usize| *self.at(i, j) as f64;
        let a = v(i, j) + (v(i1, j) - v(i, j)) * tx;
        let b = v(i, j1) + (v(i1, j1) - v(i, j1)) * tx;
        a + (b - a) * ty
    }
}

/// What covers a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cover {
    Land,
    Sea,
    Lake,
    /// River `n`.
    River(u8),
}

impl Cover {
    pub fn is_water(self) -> bool {
        self != Cover::Land
    }
}

/// A river line, from upstream to downstream, with a width per point.
#[derive(Clone, Debug, PartialEq)]
pub struct River {
    pub pts: Vec<DVec2>,
    pub widths: Vec<f64>,
}

impl River {
    /// Distance from `p` to the river's centre line, and the width there.
    pub fn nearest(&self, p: DVec2) -> (f64, f64) {
        let mut best = (f64::MAX, 0.0);
        for k in 0..self.pts.len().saturating_sub(1) {
            let d = segment_distance(p, self.pts[k], self.pts[k + 1]);
            if d < best.0 {
                best = (d, (self.widths[k] + self.widths[k + 1]) * 0.5);
            }
        }
        best
    }
}

/// The land of one settlement.
#[derive(Clone, Debug, PartialEq)]
pub struct Land {
    /// Terrain height above sea level (m); water cells hold their surface.
    pub height: Grid<f32>,
    pub cover: Grid<Cover>,
    /// Closed rings bounding the sea (even-odd: an island's shore is a hole).
    pub sea: Vec<Vec<DVec2>>,
    /// Closed lake shores.
    pub lakes: Vec<Vec<DVec2>>,
    pub rivers: Vec<River>,
    /// Unit vector out to sea, for coastal sites.
    pub seaward: Option<DVec2>,
}

impl Land {
    pub fn generate(seed: Seed, p: &TownParams) -> Land {
        let seed = seed.child("land");
        let mut rng = seed.rng();
        let s = p.scale();
        // Growth can run far out along roads and valleys.
        let half = s * 3.0 + 800.0;
        let cell = (half / 450.0).clamp(4.0, 80.0);
        let mut cover = Grid::new(half, cell, Cover::Land);
        let far = half * 1.5;

        // --- The sea.
        let mut sea = Vec::new();
        let mut seaward = None;
        let sea_angle = rng.range(0.0, std::f64::consts::TAU);
        let coast_noise = Noise::new(seed.child("coast"));
        if p.site == Site::Island {
            // The town's island: a ragged loop around the founding point.
            let r0 = s * rng.range(1.1, 1.8) + 150.0;
            let n = 160;
            let ring: Vec<DVec2> = (0..n)
                .map(|k| {
                    let a = k as f64 / n as f64 * std::f64::consts::TAU;
                    let d = det::dir2(a);
                    let r = r0 * (1.0 + 0.3 * coast_noise.fbm(d.x * 1.6, d.y * 1.6, 4));
                    d * r
                })
                .collect();
            let square = vec![
                DVec2::new(-far, -far),
                DVec2::new(far, -far),
                DVec2::new(far, far),
                DVec2::new(-far, far),
            ];
            sea.push(square);
            sea.push(ring);
        } else if p.site.is_coastal() {
            let out = det::dir2(p.seaward.unwrap_or(sea_angle));
            seaward = Some(out);
            let along = out.perp();
            let d0 = rng.range(0.2, 0.5) * s + 80.0;
            let d0 = p.coast_distance.unwrap_or(d0);
            // A bay: headlands reach out to either side of the town.
            let (bay_depth, bay_w) = if p.site == Site::Bay {
                (
                    s * rng.range(0.6, 1.4) + 200.0,
                    s * rng.range(0.6, 1.2) + 200.0,
                )
            } else {
                (0.0, 1.0)
            };
            let wave_l = (s * 0.8).max(300.0);
            let amp = s * rng.range(0.08, 0.22) + 30.0;
            let step = (cell * 1.5).max(10.0);
            let n = (far / step).ceil() as i64;
            let mut line: Vec<DVec2> = (-n..=n)
                .map(|k| {
                    let t = k as f64 * step;
                    let x = t / bay_w;
                    let off = d0
                        + amp * coast_noise.fbm(t / wave_l, 0.5, 5)
                        + bay_depth * (1.0 - det::exp(-x * x));
                    along * t + out * off
                })
                .collect();
            // Close the ring on the sea side.
            line.push(along * (n as f64 * step) + out * far * 2.0);
            line.push(-along * (n as f64 * step) + out * far * 2.0);
            sea.push(line);
        }

        // --- A lake beside the town.
        let mut lakes = Vec::new();
        if p.site == Site::LakeShore {
            let dir = det::dir2(rng.range(0.0, std::f64::consts::TAU));
            let r0 = s * rng.range(0.8, 2.5) + 300.0;
            let c = dir * (r0 * 1.7 + s * rng.range(0.1, 0.4) + 50.0);
            let ln = Noise::new(seed.child("lake"));
            let n = 120;
            lakes.push(
                (0..n)
                    .map(|k| {
                        let a = k as f64 / n as f64 * std::f64::consts::TAU;
                        let d = det::dir2(a);
                        c + DVec2::new(d.x * 1.3, d.y * 0.8)
                            * r0
                            * (1.0 + 0.25 * ln.fbm(d.x * 1.4, d.y * 1.4, 4))
                    })
                    .collect(),
            );
        }
        // On a planet the sea is the planet's: every cell whose ground lies
        // below sea level, so the town's shore is the real, ragged coast.
        let planet_ground: Option<Vec<f32>> = p.planet.as_ref().map(|planet| {
            let (w, origin) = (cover.w, cover.origin);
            crate::planet::grid::par_map(cover.w * cover.h, |k| {
                let q = origin + DVec2::new((k % w) as f64 + 0.5, (k / w) as f64 + 0.5) * cell;
                planet.ground(q) as f32
            })
        });
        if let Some(ground) = &planet_ground {
            let mut any = false;
            for (k, g) in ground.iter().enumerate() {
                if *g < 0.0 {
                    cover.data[k] = Cover::Sea;
                    any = true;
                }
            }
            // The rings only say "there is a sea" here; the raster is the truth.
            sea.clear();
            if any {
                sea.push(vec![
                    DVec2::new(-far, -far),
                    DVec2::new(far, -far),
                    DVec2::new(far, far),
                    DVec2::new(-far, far),
                ]);
            } else {
                seaward = None;
            }
        } else {
            fill_rings(&mut cover, &sea, Cover::Sea);
        }
        fill_rings(&mut cover, &lakes, Cover::Lake);

        // --- Rivers.
        let mut rivers = Vec::new();
        if p.rivers > 0 {
            let flow = match seaward {
                Some(out) if p.site == Site::Estuary || rng.chance(0.8) => {
                    det::dir2(det::atan2(out.y, out.x) + rng.range(-0.5, 0.5))
                }
                _ => match lakes.first() {
                    // Into the lake.
                    Some(l) => centroid(l).normalize(),
                    None => det::dir2(rng.range(0.0, std::f64::consts::TAU)),
                },
            };
            // The planet may fix which way the river runs.
            let flow = p.flow.map_or(flow, det::dir2);
            let w = p.river_width;
            // The town grew beside the river, not in it.
            let side = flow.perp() * if rng.chance(0.5) { 1.0 } else { -1.0 };
            // A hill town looks down on its river from a distance.
            let below = if p.site == Site::Hilltop {
                s * 0.7 + 100.0
            } else {
                0.0
            };
            let through = side
                * ((w * 0.5).max(cell * 0.55)
                    + cell
                    + rng.range(30.0, 60.0)
                    + s * rng.range(0.0, 0.12)
                    + below);
            let mut r = river_through(
                &mut rng,
                &seed.child("river0"),
                through,
                flow,
                w,
                half,
                &cover,
            );
            if p.site == Site::Estuary {
                widen_mouth(&mut r, &cover);
            }
            rivers.push(r);
            for k in 1..p.rivers as usize {
                // A tributary joins upstream of the town, from one side.
                let main = &rivers[0];
                let mid = nearest_index(&main.pts, DVec2::ZERO);
                let back = ((s * rng.range(0.1, 0.6) + 100.0) / (w.max(cell))).round() as usize;
                let j = mid.saturating_sub(back.max(2)).max(1);
                let at = main.pts[j];
                let down = (main.pts[j + 1] - main.pts[j - 1]).normalize();
                let sign = if (k % 2 == 1) == rng.chance(0.5) {
                    1.0
                } else {
                    -1.0
                };
                let a = det::atan2(-down.y, -down.x) + sign * rng.range(0.6, 1.4);
                let aw = w * rng.range(0.35, 0.75);
                let mut arm = river_from(
                    &mut rng,
                    &seed.child("river").child(k),
                    at,
                    det::dir2(a),
                    aw,
                    half,
                );
                arm.pts.reverse();
                arm.widths.reverse();
                rivers.push(arm);
            }
        }
        for (k, r) in rivers.iter().enumerate() {
            stamp_river(&mut cover, r, k as u8);
        }

        // --- Terrain, falling to the water.
        let to_water = distance_to(&cover, |c| c.is_water());
        let relief_n = Noise::new(seed.child("relief"));
        let amp = 4.0 + p.relief * 60.0 + p.relief * p.relief * 200.0;
        let wl = (s * 0.9).clamp(500.0, 7000.0);
        let valley_axis = (p.site == Site::Valley).then(|| {
            let a = match rivers.first() {
                Some(r) => {
                    let k = nearest_index(&r.pts, DVec2::ZERO).clamp(1, r.pts.len() - 2);
                    r.pts[k + 1] - r.pts[k - 1]
                }
                None => det::dir2(rng.range(0.0, std::f64::consts::TAU)),
            };
            (a.normalize(), s * rng.range(0.25, 0.6) + 120.0)
        });
        let hill = (p.site == Site::Hilltop).then(|| {
            (
                det::dir2(rng.range(0.0, std::f64::consts::TAU)) * s * rng.range(0.0, 0.15),
                // Standing well above the hills around it.
                30.0 + amp * 0.7,
                s * rng.range(0.4, 0.6) + 100.0,
            )
        });
        let mut height = Grid::new(half, cell, 0.0f32);
        for j in 0..cover.h {
            for i in 0..cover.w {
                if cover.at(i, j).is_water() {
                    continue;
                }
                let q = cover.center(i, j);
                let n = relief_n.fbm(q.x / wl, q.y / wl, 5) * 0.5 + 0.5;
                let rough = if p.relief > 0.55 {
                    relief_n.ridged(q.x / wl * 0.7 + 9.0, q.y / wl * 0.7, 4)
                } else {
                    n
                };
                let m = n * 0.6 + rough * 0.4;
                let mut h = match &planet_ground {
                    // The planet's own hills.
                    Some(ground) => (ground[j * cover.w + i] as f64).max(0.0),
                    None => amp * m * m,
                };
                if let Some((axis, vw)) = valley_axis {
                    // Valley sides climbing away from the axis.
                    let d = axis.perp_dot(q).abs();
                    h = h * smooth(vw * 0.5, vw * 1.8, d)
                        + (amp * 0.8 + 40.0) * smooth(vw * 0.7, vw * 2.5, d);
                }
                if let Some((c, top, r)) = hill {
                    // A broad, flat-topped hill the old town sits on, its
                    // own bumps smoothed away near the top.
                    let x = q.distance(c) / r;
                    let x2 = x * x;
                    let plateau = det::exp(-x2 * x2);
                    h = h * (1.0 - 0.8 * plateau) + top * plateau;
                }
                // Banks a little above the water, rising to the hills.
                let d = *to_water.at(i, j) as f64;
                // (On a planet's higher ground the sides are no steeper than 1 in 3.)
                let reach = (120.0 + p.relief * 500.0).max(if planet_ground.is_some() {
                    3.0 * h
                } else {
                    0.0
                });
                let rise = smooth(0.0, reach, d);
                h = 1.0 + 2.0 * smooth(0.0, 40.0, d) + h * rise;
                height.set(i, j, h as f32);
            }
        }
        Land {
            height,
            cover,
            sea,
            lakes,
            rivers,
            seaward,
        }
    }

    pub fn cover_at(&self, p: DVec2) -> Cover {
        self.cover
            .cell_of(p)
            .map_or(Cover::Land, |(i, j)| *self.cover.at(i, j))
    }

    pub fn height_at(&self, p: DVec2) -> f64 {
        self.height.sample(p)
    }
}

/// Smoothstep from `a` to `b`.
pub fn smooth(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn centroid(pts: &[DVec2]) -> DVec2 {
    pts.iter().copied().sum::<DVec2>() / pts.len() as f64
}

fn nearest_index(pts: &[DVec2], p: DVec2) -> usize {
    let mut best = (f64::MAX, 0);
    for (k, q) in pts.iter().enumerate() {
        let d = q.distance_squared(p);
        if d < best.0 {
            best = (d, k);
        }
    }
    best.1
}

/// A meandering river line through `through`, flowing along `flow`.
fn river_through(
    rng: &mut Rng,
    seed: &Seed,
    through: DVec2,
    flow: DVec2,
    width: f64,
    half: f64,
    cover: &Grid<Cover>,
) -> River {
    let down = river_from(rng, &seed.child("down"), through, flow, width, half);
    let up = river_from(rng, &seed.child("up"), through, -flow, width, half);
    let mut pts: Vec<DVec2> = up.pts.into_iter().rev().collect();
    pts.extend(down.pts.into_iter().skip(1));
    // Stop shortly after it reaches the sea or a lake.
    let mut end = pts.len();
    let mut wet = 0;
    for (k, q) in pts.iter().enumerate() {
        if k > pts.len() / 2
            && cover
                .cell_of(*q)
                .is_some_and(|(i, j)| cover.at(i, j).is_water())
        {
            wet += 1;
            if wet > 3 {
                end = k + 1;
                break;
            }
        }
    }
    pts.truncate(end);
    let widths = vec![width; pts.len()];
    River { pts, widths }
}

/// A river line from `start` heading `dir` until it leaves the land square.
/// Meanders are about a dozen river widths long, as in nature.
fn river_from(
    rng: &mut Rng,
    seed: &Seed,
    start: DVec2,
    dir: DVec2,
    width: f64,
    half: f64,
) -> River {
    let n = Noise::new(*seed);
    let base = det::atan2(dir.y, dir.x);
    let meander = (width * 12.0).max(200.0);
    let step = (width * 0.8).clamp(8.0, meander / 8.0);
    let swing = rng.range(0.5, 1.1);
    let drift = Noise::new(seed.child("drift"));
    let mut pts = vec![start];
    let mut p = start;
    let mut s = 0.0;
    while p.abs().max_element() < half * 1.05 && pts.len() < 4000 {
        // Small meanders on a slow drift of the valley's direction.
        let a = base
            + swing * n.fbm(s / meander, 0.3, 3)
            + 0.6 * drift.fbm(s / (meander * 8.0), 0.7, 2);
        p += det::dir2(a) * step;
        s += step;
        pts.push(p);
    }
    let widths = vec![width; pts.len()];
    River { pts, widths }
}

/// An estuary: the last stretch before the sea opens out like a funnel.
fn widen_mouth(r: &mut River, cover: &Grid<Cover>) {
    let n = r.pts.len();
    let mouth = r
        .pts
        .iter()
        .position(|q| {
            cover
                .cell_of(*q)
                .is_some_and(|(i, j)| *cover.at(i, j) == Cover::Sea)
        })
        .unwrap_or(n - 1);
    let w = r.widths[0];
    let reach = (w * 25.0).max(400.0);
    let mut s = 0.0;
    for k in (0..mouth.min(n)).rev() {
        if k + 1 < n {
            s += r.pts[k].distance(r.pts[k + 1]);
        }
        let t = (1.0 - s / reach).max(0.0);
        r.widths[k] = w * (1.0 + 2.5 * t * t);
    }
    for k in mouth..n {
        r.widths[k] = w * 3.5;
    }
}

/// Marks the cells under a river (at least one cell wide, so it never
/// breaks up in the raster).
fn stamp_river(g: &mut Grid<Cover>, r: &River, id: u8) {
    for k in 0..r.pts.len().saturating_sub(1) {
        let (a, b) = (r.pts[k], r.pts[k + 1]);
        let hw = (r.widths[k].max(r.widths[k + 1]) * 0.5).max(g.cell * 0.55);
        let lo = a.min(b) - DVec2::splat(hw);
        let hi = a.max(b) + DVec2::splat(hw);
        let (Some((i0, j0)), Some((i1, j1))) = (
            g.cell_of(lo.max(g.origin)),
            g.cell_of(hi.min(g.origin + DVec2::splat(g.half() * 2.0 - 1e-6))),
        ) else {
            continue;
        };
        for j in j0..=j1 {
            for i in i0..=i1 {
                if *g.at(i, j) == Cover::Land && segment_distance(g.center(i, j), a, b) <= hw {
                    g.set(i, j, Cover::River(id));
                }
            }
        }
    }
}

/// Fills closed rings (even-odd) with `v`, scanline by scanline.
fn fill_rings(g: &mut Grid<Cover>, rings: &[Vec<DVec2>], v: Cover) {
    if rings.is_empty() {
        return;
    }
    let mut xs = Vec::new();
    for j in 0..g.h {
        let y = g.center(0, j).y;
        xs.clear();
        for ring in rings {
            for k in 0..ring.len() {
                let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
                if (a.y <= y) != (b.y <= y) {
                    xs.push(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x));
                }
            }
        }
        xs.sort_by(f64::total_cmp);
        for pair in xs.chunks(2) {
            let [x0, x1] = pair else { continue };
            for i in 0..g.w {
                let x = g.center(i, j).x;
                if x >= *x0 && x < *x1 {
                    g.set(i, j, v);
                }
            }
        }
    }
}

/// Distance (m) from every cell to the nearest cell where `target` holds
/// (two-pass chamfer; close to Euclidean).
pub fn distance_to(g: &Grid<Cover>, target: impl Fn(Cover) -> bool) -> Grid<f32> {
    let (w, h) = (g.w, g.h);
    let big = f32::MAX / 4.0;
    let mut d = Grid {
        origin: g.origin,
        cell: g.cell,
        w,
        h,
        data: g
            .data
            .iter()
            .map(|c| if target(*c) { 0.0 } else { big })
            .collect(),
    };
    let (a, b) = (g.cell as f32, g.cell as f32 * std::f32::consts::SQRT_2);
    for j in 0..h {
        for i in 0..w {
            let mut v = *d.at(i, j);
            if i > 0 {
                v = v.min(d.at(i - 1, j) + a);
            }
            if j > 0 {
                v = v.min(d.at(i, j - 1) + a);
                if i > 0 {
                    v = v.min(d.at(i - 1, j - 1) + b);
                }
                if i + 1 < w {
                    v = v.min(d.at(i + 1, j - 1) + b);
                }
            }
            d.set(i, j, v);
        }
    }
    for j in (0..h).rev() {
        for i in (0..w).rev() {
            let mut v = *d.at(i, j);
            if i + 1 < w {
                v = v.min(d.at(i + 1, j) + a);
            }
            if j + 1 < h {
                v = v.min(d.at(i, j + 1) + a);
                if i + 1 < w {
                    v = v.min(d.at(i + 1, j + 1) + b);
                }
                if i > 0 {
                    v = v.min(d.at(i - 1, j + 1) + b);
                }
            }
            d.set(i, j, v);
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn the_founding_point_is_dry_and_water_matches_the_site() {
        for s in 0..40 {
            let pop = det::powf(10.0, 2.0 + (s % 5) as f64);
            let p = TownParams::sample(Seed(s), pop);
            let land = Land::generate(Seed(s), &p);
            assert_eq!(
                land.cover_at(DVec2::ZERO),
                Cover::Land,
                "seed {s} {:?}",
                p.site
            );
            assert_eq!(land.rivers.len(), p.rivers as usize, "seed {s}");
            let sea = land.cover.data.iter().filter(|c| **c == Cover::Sea).count();
            assert_eq!(sea > 0, p.site.is_coastal(), "seed {s} {:?}", p.site);
            assert!(land.height.data.iter().all(|h| h.is_finite() && *h >= 0.0));
        }
    }

    #[test]
    fn same_seed_same_land() {
        let p = TownParams::sample(Seed(5), 2e5);
        assert_eq!(Land::generate(Seed(5), &p), Land::generate(Seed(5), &p));
    }
}
