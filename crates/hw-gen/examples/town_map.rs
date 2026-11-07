//! Draws a settlement from the new generator as an SVG map.
//!
//! Usage: cargo run --release -p hw-gen --example town_map -- [seed] [population|preset] [out.svg]
//!
//! `population` is a number (50 for a hamlet, 1e7 for a megalopolis); a
//! preset is one of: hill, colonial, mill, baroque, garden, mega.

use hw_core::Seed;
use hw_gen::urban::{CentreKind, Cover, Era, Growth, Land, Network, Preset, RoadClass, TownParams};
use std::fmt::Write;

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let what = args.next().unwrap_or_else(|| "200000".into());
    let out = args
        .next()
        .unwrap_or_else(|| "screenshots/town_map.svg".into());
    let params = match what.as_str() {
        "hill" => Preset::MedievalHillTown.params(Seed(seed)),
        "colonial" => Preset::ColonialGridPort.params(Seed(seed)),
        "mill" => Preset::RiverValleyMillTown.params(Seed(seed)),
        "baroque" => Preset::BaroqueCapital.params(Seed(seed)),
        "garden" => Preset::GardenCity.params(Seed(seed)),
        "mega" => Preset::SprawlingMegalopolis.params(Seed(seed)),
        n => TownParams::sample(Seed(seed), n.parse().expect("population or preset")),
    };
    let t = std::time::Instant::now();
    let land = Land::generate(Seed(seed), &params);
    println!("land in {:.2} s", t.elapsed().as_secs_f64());
    let t = std::time::Instant::now();
    let growth = Growth::generate(Seed(seed), &params, &land);
    println!("growth in {:.2} s", t.elapsed().as_secs_f64());
    let t = std::time::Instant::now();
    let net = Network::generate(Seed(seed), &params, &land, &growth);
    println!(
        "main roads in {:.2} s: {} roads, {} bridges",
        t.elapsed().as_secs_f64(),
        net.roads.len(),
        net.bridges.len()
    );
    println!("{params:?}");

    let g = &land.height;
    let half = g.half();
    let mut svg = String::new();
    let _ = writeln!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{} {} {} {}" width="1600" height="1600">"#,
        -half,
        -half,
        2.0 * half,
        2.0 * half
    );
    let _ = writeln!(
        svg,
        r#"<image x="{}" y="{}" width="{}" height="{}" href="data:image/png;base64,{}"/>"#,
        -half,
        -half,
        2.0 * half,
        2.0 * half,
        base64(&png(g.w, g.h, &terrain_rgb(&land, &growth)))
    );
    let lw = half / 400.0;
    for r in &land.rivers {
        for k in 0..r.pts.len() - 1 {
            let (a, b) = (r.pts[k], r.pts[k + 1]);
            let _ = writeln!(
                svg,
                r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#4f86b8" stroke-width="{:.1}" stroke-linecap="round"/>"##,
                a.x,
                a.y,
                b.x,
                b.y,
                r.widths[k].max(lw)
            );
        }
    }
    for b in &net.bridges {
        let _ = writeln!(
            svg,
            r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#222" stroke-width="{:.1}"/>"##,
            b[0].x,
            b[0].y,
            b[1].x,
            b[1].y,
            lw * 5.0
        );
    }
    for r in &net.roads {
        let pts: Vec<String> = r
            .pts
            .iter()
            .map(|q| format!("{:.1},{:.1}", q.x, q.y))
            .collect();
        let (col, w) = match r.class {
            RoadClass::Country => ("#5a4632", 1.6),
            RoadClass::Boulevard => ("#b01818", 3.0),
            RoadClass::Avenue => ("#303030", 1.8),
            RoadClass::Quay => ("#1a5a8a", 2.0),
        };
        let _ = writeln!(
            svg,
            r##"<polyline points="{}" fill="none" stroke="{col}" stroke-width="{:.1}" stroke-linejoin="round" stroke-linecap="round"/>"##,
            pts.join(" "),
            lw * w
        );
    }
    for c in &growth.centres {
        let (fill, r) = match c.kind {
            CentreKind::Core => ("#c0102a", 6.0),
            CentreKind::Village => ("#7a2a8a", 4.0),
            CentreKind::Business => ("#1040c0", 5.0),
        };
        let _ = writeln!(
            svg,
            r##"<circle cx="{:.1}" cy="{:.1}" r="{:.1}" fill="{fill}" stroke="#fff" stroke-width="{:.1}"/>"##,
            c.pos.x,
            c.pos.y,
            lw * r,
            lw
        );
    }
    // Scale bar: 1 km (or 100 m for small places).
    let bar = if half > 3000.0 { 1000.0 } else { 100.0 };
    let (x0, y0) = (-half * 0.95, half * 0.93);
    let _ = writeln!(
        svg,
        r##"<line x1="{x0}" y1="{y0}" x2="{}" y2="{y0}" stroke="#000" stroke-width="{}"/><text x="{x0}" y="{}" font-size="{}" font-family="sans-serif">{} m</text>"##,
        x0 + bar,
        lw * 3.0,
        y0 - lw * 6.0,
        half / 40.0,
        bar
    );
    let _ = writeln!(
        svg,
        r##"<text x="{}" y="{}" font-size="{}" font-family="sans-serif" fill="#000">seed {seed}: {:?}, {} people, relief {:.2}</text>"##,
        -half * 0.95,
        -half * 0.92,
        half / 35.0,
        params.site,
        params.population as u64,
        params.relief
    );
    svg.push_str("</svg>\n");
    if let Some(dir) = std::path::Path::new(&out).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(&out, svg).expect("write svg");
    println!("-> {out}");
}

/// Shaded relief: green lowlands, brown hills, grey heights; water blue.
fn terrain_rgb(land: &Land, growth: &Growth) -> Vec<u8> {
    let g = &land.height;
    let mut px = Vec::with_capacity(g.w * g.h * 3);
    let top = g.data.iter().fold(1.0f32, |a, b| a.max(*b)) as f64;
    for j in 0..g.h {
        for i in 0..g.w {
            let c = match land.cover.at(i, j) {
                Cover::Sea => [52.0, 92.0, 140.0],
                Cover::Lake => [70.0, 120.0, 165.0],
                Cover::River(_) => [79.0, 134.0, 184.0],
                Cover::Land => {
                    let h = *g.at(i, j) as f64;
                    let t = (h / top.max(60.0)).clamp(0.0, 1.0);
                    let low = [128.0, 166.0, 104.0];
                    let mid = [176.0, 160.0, 112.0];
                    let high = [205.0, 200.0, 195.0];
                    let base = if t < 0.5 {
                        lerp3(low, mid, t * 2.0)
                    } else {
                        lerp3(mid, high, (t - 0.5) * 2.0)
                    };
                    // Light from the north-west.
                    let hx = *g.at((i + 1).min(g.w - 1), j) as f64
                        - *g.at(i.saturating_sub(1), j) as f64;
                    let hy = *g.at(i, (j + 1).min(g.h - 1)) as f64
                        - *g.at(i, j.saturating_sub(1)) as f64;
                    let shade = (1.0 - (hx + hy) / (2.0 * g.cell) * 1.5).clamp(0.55, 1.35);
                    let base = if growth.built(i, j) {
                        let town = match growth.era(i, j) {
                            Era::Old => [150.0, 70.0, 60.0],
                            Era::Extension => [215.0, 150.0, 95.0],
                            Era::Modern => [232.0, 214.0, 170.0],
                        };
                        lerp3(base, town, 0.8)
                    } else {
                        base
                    };
                    [base[0] * shade, base[1] * shade, base[2] * shade]
                }
            };
            px.extend(c.iter().map(|v| v.clamp(0.0, 255.0) as u8));
        }
    }
    px
}

fn lerp3(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// An uncompressed RGB PNG.
fn png(w: usize, h: usize, rgb: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity((w * 3 + 1) * h);
    for row in rgb.chunks(w * 3) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (k, b) in blocks.iter().enumerate() {
        z.push((k + 1 == blocks.len()) as u8);
        let n = b.len() as u16;
        z.extend_from_slice(&n.to_le_bytes());
        z.extend_from_slice(&(!n).to_le_bytes());
        z.extend_from_slice(b);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for v in &raw {
        a = (a + *v as u32) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = 0xffff_ffffu32;
    for v in &out[start..] {
        crc ^= *v as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    out.extend_from_slice(&(!crc).to_be_bytes());
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len() * 4 / 3 + 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        s.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    s
}
