//! Draw the planet of a seed: `cargo run --release -p hw-gen --example planet_map -- <seed> <out-prefix> [n]`.
//!
//! Writes `<prefix>-world.png` (the whole planet, biomes and shading, rivers
//! and lakes), `<prefix>-rain.png`, `<prefix>-height.png` and three globe
//! views `<prefix>-globe0..2.png`, and prints the landmasses and the biomes.
//! `n` is the grid size per cube face (default 512, cells of about 20 km).

use glam::{DVec3, Vec3};
use hw_core::{det, Seed};
use hw_gen::planet::{biome::classify, Biome, Cell, PlanetMap};
use std::io::Write;

const W: usize = 1600;
const H: usize = 800;

struct Image {
    w: usize,
    h: usize,
    px: Vec<[u8; 3]>,
}

impl Image {
    fn new(w: usize, h: usize) -> Self {
        Image {
            w,
            h,
            px: vec![[0, 0, 0]; w * h],
        }
    }

    fn put(&mut self, x: i64, y: i64, c: [u8; 3]) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.px[y as usize * self.w + x as usize] = c;
        }
    }

    fn line(&mut self, a: (f64, f64), b: (f64, f64), width: f64, c: [u8; 3]) {
        let n = ((b.0 - a.0).abs().max((b.1 - a.1).abs()).ceil() as usize).max(1) * 2;
        let r = (width * 0.5).max(0.5);
        for k in 0..=n {
            let t = k as f64 / n as f64;
            let (x, y) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            for dy in -(r.ceil() as i64)..=(r.ceil() as i64) {
                for dx in -(r.ceil() as i64)..=(r.ceil() as i64) {
                    if ((dx * dx + dy * dy) as f64) <= r * r + 0.3 {
                        self.put(x.round() as i64 + dx, y.round() as i64 + dy, c);
                    }
                }
            }
        }
    }

    /// A PNG with stored (uncompressed) blocks: big, but no dependency.
    fn save(&self, path: &str) {
        let mut raw = Vec::with_capacity((self.w * 3 + 1) * self.h);
        for y in 0..self.h {
            raw.push(0);
            for x in 0..self.w {
                raw.extend_from_slice(&self.px[y * self.w + x]);
            }
        }
        let mut z = vec![0x78, 0x01];
        let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
        for (i, b) in blocks.iter().enumerate() {
            z.push((i + 1 == blocks.len()) as u8);
            z.extend_from_slice(&(b.len() as u16).to_le_bytes());
            z.extend_from_slice(&(!(b.len() as u16)).to_le_bytes());
            z.extend_from_slice(b);
        }
        let (mut a, mut b) = (1u32, 0u32);
        for &v in &raw {
            a = (a + v as u32) % 65521;
            b = (b + a) % 65521;
        }
        z.extend_from_slice(&((b << 16) | a).to_be_bytes());
        let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&(self.w as u32).to_be_bytes());
        ihdr.extend_from_slice(&(self.h as u32).to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        chunk(&mut out, b"IHDR", &ihdr);
        chunk(&mut out, b"IDAT", &z);
        chunk(&mut out, b"IEND", &[]);
        std::fs::File::create(path)
            .and_then(|mut f| f.write_all(&out))
            .expect("write png");
    }
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = 0xffff_ffffu32;
    for &b in &out[start..] {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    out.extend_from_slice(&(!crc).to_be_bytes());
}

#[derive(Clone, Copy, PartialEq)]
enum Layer {
    Biome,
    Rain,
    Height,
    Aether,
}

/// Lon/lat (radians) to a unit vector, longitude 0 on +Z, growing to +X.
fn unit(lat: f64, lon: f64) -> DVec3 {
    let (sl, cl) = det::sin_cos(lat);
    let (so, co) = det::sin_cos(lon);
    DVec3::new(cl * so, sl, cl * co)
}

fn shade(c: [u8; 3], k: f64) -> [u8; 3] {
    [0, 1, 2].map(|i| (c[i] as f64 * k).clamp(0.0, 255.0) as u8)
}

fn mix(a: [u8; 3], b: [u8; 3], t: f64) -> [u8; 3] {
    [0, 1, 2].map(|i| (a[i] as f64 + (b[i] as f64 - a[i] as f64) * t) as u8)
}

fn ramp(stops: &[(f64, [u8; 3])], x: f64) -> [u8; 3] {
    if x <= stops[0].0 {
        return stops[0].1;
    }
    for w in stops.windows(2) {
        if x <= w[1].0 {
            return mix(w[0].1, w[1].1, (x - w[0].0) / (w[1].0 - w[0].0));
        }
    }
    stops[stops.len() - 1].1
}

/// Colour of a pixel (unshaded) from the direction and the height there.
fn base_color(map: &PlanetMap, u: DVec3, h: f64, layer: Layer) -> [u8; 3] {
    let s = map.sample(u);
    if h <= 0.0 && layer != Layer::Aether {
        let deep = ramp(
            &[
                (-6000.0, [10, 28, 70]),
                (-1500.0, [22, 58, 110]),
                (-100.0, [60, 120, 170]),
                (0.0, [90, 160, 200]),
            ],
            h,
        );
        return match layer {
            Layer::Rain => [20, 20, 30],
            _ => deep,
        };
    }
    match layer {
        Layer::Rain => ramp(
            &[
                (0.0, [200, 170, 110]),
                (400.0, [200, 200, 120]),
                (1000.0, [90, 170, 90]),
                (2000.0, [30, 110, 150]),
                (4000.0, [20, 40, 120]),
            ],
            s.rain as f64,
        ),
        Layer::Aether => ramp(
            &[
                (0.0, [20, 20, 60]),
                (0.3, [60, 50, 140]),
                (0.6, [150, 70, 200]),
                (0.85, [255, 150, 230]),
                (1.0, [255, 255, 255]),
            ],
            map.globe.aether(u),
        ),
        Layer::Height => ramp(
            &[
                (0.0, [60, 140, 70]),
                (500.0, [150, 170, 80]),
                (1500.0, [160, 120, 80]),
                (3500.0, [130, 110, 100]),
                (6000.0, [250, 250, 250]),
            ],
            h,
        ),
        Layer::Biome => {
            let biome = if map.lake[s.cell as usize] > 0.0 {
                Biome::Lake
            } else {
                classify(s.temp as f64, s.rain as f64, h)
            };
            biome.color()
        }
    }
}

fn rivers(map: &PlanetMap, img: &mut Image, project: &dyn Fn(Vec3) -> Option<(f64, f64)>) {
    for c in 0..map.grid.cells() {
        if !map.is_river(c as Cell) || map.flow[c] < 120.0 {
            continue;
        }
        let to = map.flow_to[c];
        if to == Cell::MAX {
            continue;
        }
        let (Some(a), Some(b)) = (project(map.centers[c]), project(map.centers[to as usize]))
        else {
            continue;
        };
        if (a.0 - b.0).abs() > img.w as f64 / 4.0 {
            continue; // over the edge of the map
        }
        let width = (0.6 + (map.flow[c] as f64 / 60.0).ln().max(0.0) * 0.55).min(4.0);
        img.line(a, b, width, [70, 130, 210]);
    }
}

fn render(
    map: &PlanetMap,
    layer: Layer,
    finest: f64,
    w: usize,
    h: usize,
    dir: &(dyn Fn(usize, usize) -> Option<DVec3> + Sync),
    project: &dyn Fn(Vec3) -> Option<(f64, f64)>,
) -> Image {
    // Heights first, then colour, then light from the upper left.
    let cells: Vec<Option<(DVec3, f64)>> = hw_gen::planet::grid::par_map(w * h, |i| {
        dir(i % w, i / w).map(|u| (u, map.globe.elevation(u, finest)))
    });
    let px: Vec<[u8; 3]> = hw_gen::planet::grid::par_map(w * h, |i| {
        let Some((u, height)) = cells[i] else {
            return [6, 8, 14];
        };
        let (x, y) = (i % w, i / w);
        let at = |x: usize, y: usize| cells[y * w + x].map_or(height, |c| c.1);
        let (xm, ym) = (x.saturating_sub(1), y.saturating_sub(1));
        let (xp, yp) = ((x + 1).min(w - 1), (y + 1).min(h - 1));
        // Metres of rise towards the lower right, over a pixel or two.
        let slope = (at(xp, yp) - at(xm, ym)) / 1000.0;
        let lit = if height > 0.0 {
            (1.0 - slope * 0.35).clamp(0.6, 1.4)
        } else {
            1.0
        };
        shade(base_color(map, u, height, layer), lit)
    });
    let mut img = Image::new(w, h);
    img.px = px;
    if layer == Layer::Biome {
        rivers(map, &mut img, project);
    }
    for wonder in &map.wonders {
        if let Some((x, y)) = project(wonder.at.as_vec3()) {
            for (dx, dy) in [
                (-3, 0),
                (3, 0),
                (0, -3),
                (0, 3),
                (0, 0),
                (-2, -2),
                (2, 2),
                (-2, 2),
                (2, -2),
            ] {
                img.put(x as i64 + dx, y as i64 + dy, [255, 215, 0]);
            }
            img.put(x as i64, y as i64, [200, 0, 0]);
        }
    }
    img
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let seed: u64 = args.first().and_then(|s| s.parse().ok()).unwrap_or(1);
    let prefix = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "screenshots/planet".into());
    let n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(512);
    let t0 = std::time::Instant::now();
    let map = PlanetMap::build(Seed(seed), n);
    println!(
        "solved the planet of seed {seed} on {} cells in {:.2?}",
        map.grid.cells(),
        t0.elapsed()
    );
    println!("land share {:.1}%", map.land_share() * 100.0);
    for (k, m) in map.landmasses().iter().take(10).enumerate() {
        let ll = hw_core::LatLon::from_dir(m.centre);
        println!(
            "  landmass {k}: {:>10.0} km² around {:>5.1}°N {:>6.1}°E",
            m.area_km2,
            ll.lat.to_degrees(),
            ll.lon.to_degrees()
        );
    }
    for w in &map.wonders {
        let ll = hw_core::LatLon::from_dir(w.at);
        println!(
            "  wonder {:<28} {:<18} {:>5.1}°N {:>6.1}°E",
            w.name,
            w.kind.label(),
            ll.lat.to_degrees(),
            ll.lon.to_degrees()
        );
    }
    let rivers_n = (0..map.grid.cells())
        .filter(|c| map.is_river(*c as Cell))
        .count();
    let lakes_n = map.lake.iter().filter(|d| **d > 0.0).count();
    let biggest = map.flow.iter().cloned().fold(0.0f32, f32::max);
    println!("{rivers_n} river cells, {lakes_n} lake cells, biggest river {biggest:.0} m³/s");
    let mut rains: Vec<f32> = (0..map.grid.cells())
        .filter(|c| map.elev[*c] > 0.0)
        .map(|c| map.rain[c])
        .collect();
    rains.sort_by(f32::total_cmp);
    let q = |f: f64| rains[((rains.len() - 1) as f64 * f) as usize];
    println!(
        "land rain mm: p10 {:.0} p25 {:.0} p50 {:.0} p75 {:.0} p90 {:.0} max {:.0}",
        q(0.1),
        q(0.25),
        q(0.5),
        q(0.75),
        q(0.9),
        q(1.0)
    );
    let mut hs: Vec<f32> = (0..map.grid.cells())
        .filter(|c| map.elev[*c] > 0.0)
        .map(|c| map.elev[c])
        .collect();
    hs.sort_by(f32::total_cmp);
    println!(
        "land height m: p10 {:.0} p50 {:.0} p90 {:.0} p99 {:.0} max {:.0}",
        hs[hs.len() / 10],
        hs[hs.len() / 2],
        hs[hs.len() * 9 / 10],
        hs[hs.len() * 99 / 100],
        hs[hs.len() - 1]
    );
    let land = (0..map.grid.cells()).filter(|c| map.elev[*c] > 0.0).count() as f64;
    for b in Biome::ALL {
        let k = map.biome.iter().filter(|x| **x == b).count();
        if b != Biome::Ocean && k > 0 {
            println!(
                "  {:>16}: {:>5.1}% of land",
                b.name(),
                k as f64 / land * 100.0
            );
        }
    }
    let equirect_dir = |x: usize, y: usize| {
        let lon = (x as f64 + 0.5) / W as f64 * std::f64::consts::TAU - std::f64::consts::PI;
        let lat = std::f64::consts::FRAC_PI_2 - (y as f64 + 0.5) / H as f64 * std::f64::consts::PI;
        Some(unit(lat, lon))
    };
    let equirect_xy = |p: Vec3| {
        let d = p.as_dvec3();
        let (lat, lon) = (det::asin(d.y.clamp(-1.0, 1.0)), det::atan2(d.x, d.z));
        Some((
            (lon + std::f64::consts::PI) / std::f64::consts::TAU * W as f64,
            (std::f64::consts::FRAC_PI_2 - lat) / std::f64::consts::PI * H as f64,
        ))
    };
    let finest = 12_000.0;
    for (name, layer) in [
        ("world", Layer::Biome),
        ("rain", Layer::Rain),
        ("height", Layer::Height),
        ("aether", Layer::Aether),
    ] {
        let img = render(&map, layer, finest, W, H, &equirect_dir, &equirect_xy);
        img.save(&format!("{prefix}-{name}.png"));
    }
    // Three globes, seen from above 20°N and three longitudes.
    let size = 900usize;
    for (k, lon) in [0.0f64, 120.0, 240.0].iter().enumerate() {
        let centre = unit(20f64.to_radians(), lon.to_radians());
        let east = DVec3::Y.cross(centre).normalize();
        let north = centre.cross(east);
        let dir = move |x: usize, y: usize| {
            let (a, b) = (
                (x as f64 + 0.5) / size as f64 * 2.0 - 1.0,
                1.0 - (y as f64 + 0.5) / size as f64 * 2.0,
            );
            let r2 = a * a + b * b;
            (r2 < 0.98).then(|| (east * a + north * b + centre * (1.0 - r2).sqrt()).normalize())
        };
        let project = move |p: Vec3| {
            let d = p.as_dvec3();
            (d.dot(centre) > 0.05).then(|| {
                (
                    (d.dot(east) + 1.0) * 0.5 * size as f64,
                    (1.0 - d.dot(north)) * 0.5 * size as f64,
                )
            })
        };
        let img = render(&map, Layer::Biome, finest, size, size, &dir, &project);
        img.save(&format!("{prefix}-globe{k}.png"));
    }
    home_view(&map, seed, &format!("{prefix}-home.png"));
    println!("maps written to {prefix}-*.png in {:.2?}", t0.elapsed());
}

/// The home region: the first town on the planet, 60 km across, with the
/// town's own land blending into the planet's ground. Water blue, land by
/// height with relief shading; the planned land's edge is drawn in red.
fn home_view(map: &PlanetMap, seed: u64, path: &str) {
    use hw_gen::planet::{choose_home, PlanetPatch};
    use hw_gen::urban::{params::TownParams, world::TownWorld};
    let patch = std::sync::Arc::new(PlanetPatch::new(map, choose_home(map)));
    let ll = hw_core::LatLon::from_dir(choose_home(map));
    println!(
        "home at {:.1}°N {:.1}°E: {:?}, seaward {:?}, relief {:.2}, river {:.0} m wide",
        ll.lat.to_degrees(),
        ll.lon.to_degrees(),
        patch.site.site,
        patch.site.seaward.map(|a| a.to_degrees().round()),
        patch.site.relief,
        patch.site.river_width
    );
    if let Some(a) = patch.site.seaward {
        let d = det::dir2(a);
        let line: Vec<String> = (0..12)
            .map(|k| format!("{:.0}", patch.ground(d * (k as f64 * 300.0))))
            .collect();
        println!(
            "coast distance {:?}; ground every 300 m seaward: {}",
            patch.site.coast_distance,
            line.join(" ")
        );
    }
    let params = TownParams::on_planet(Seed(seed), 20_000.0, &patch.site);
    let town =
        TownWorld::generate_on_or_stop(Seed(seed), params, Some(patch), &hw_core::Cancel::new())
            .expect("town");
    let (size, half_view) = (1000usize, 30_000.0f64);
    let m = 2.0 * half_view / size as f64;
    let heights: Vec<f64> = hw_gen::planet::grid::par_map(size * size, |i| {
        let (x, y) = (i % size, i / size);
        town.ground(glam::DVec2::new(
            (x as f64 + 0.5) * m - half_view,
            (y as f64 + 0.5) * m - half_view,
        ))
    });
    let mut img = Image::new(size, size);
    for y in 0..size {
        for x in 0..size {
            let h = heights[y * size + x];
            let (xp, yp) = ((x + 1).min(size - 1), (y + 1).min(size - 1));
            let slope = (heights[yp * size + xp]
                - heights[y.saturating_sub(1) * size + x.saturating_sub(1)])
                / (2.0 * m)
                * 6.0;
            let lit = (1.0 - slope).clamp(0.6, 1.4);
            let c = if h <= 0.0 {
                ramp(
                    &[
                        (-400.0, [20, 50, 110]),
                        (-5.0, [50, 110, 170]),
                        (0.0, [90, 160, 200]),
                    ],
                    h,
                )
            } else {
                shade(
                    ramp(
                        &[
                            (0.0, [190, 180, 120]),
                            (3.0, [90, 150, 70]),
                            (150.0, [70, 130, 60]),
                            (600.0, [150, 130, 90]),
                            (1500.0, [210, 210, 215]),
                        ],
                        h,
                    ),
                    lit,
                )
            };
            img.put(x as i64, y as i64, c);
        }
    }
    let half = town.half();
    let (lo, hi) = (
        ((-half + half_view) / m) as i64,
        ((half + half_view) / m) as i64,
    );
    for k in lo..=hi {
        for (a, b) in [(k, lo), (k, hi), (lo, k), (hi, k)] {
            img.put(a, b, [230, 40, 40]);
        }
    }
    img.save(path);
}
