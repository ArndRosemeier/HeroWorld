//! Where a town's harbour stands: an SVG map of the shore round it, with
//! the mole, the breakwater, the berths, the freighter's way out and back,
//! the roads and the landmarks, and a few numbers.
//!
//! ```
//! cargo run --release -p hw-gen --example harbour_map -- <seed> [people] [out.svg]
//! ```

use glam::DVec2;
use hw_core::Seed;
use hw_gen::planet::{choose_home, PlanetMap, PlanetPatch};
use hw_gen::urban::harbour::{ShipKind, BREAKWATER};
use hw_gen::urban::{params::TownParams, world::TownWorld, world::WATER_LEVEL};
use std::fmt::Write;

fn main() {
    let mut args = std::env::args().skip(1);
    let seed = Seed(args.next().and_then(|s| s.parse().ok()).unwrap_or(1));
    let people: f64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(40_000.0);
    let out = args
        .next()
        .unwrap_or_else(|| "screenshots/harbour_map.svg".into());
    let map = PlanetMap::build(seed, 192);
    let patch = std::sync::Arc::new(PlanetPatch::new(&map, choose_home(&map)));
    let params = TownParams::on_planet(seed, people, &patch);
    let t = std::time::Instant::now();
    let w = TownWorld::generate_on_or_stop(seed, params, Some(patch), &hw_core::Cancel::new())
        .expect("not cancelled");
    println!("town in {:.1} s", t.elapsed().as_secs_f64());
    let Some(h) = &w.harbour else {
        println!("no harbour (coastal: {})", w.coastal());
        return;
    };
    println!(
        "root {:.0},{:.0}  out {:.2},{:.2}  deck {:.2}  land end {:.1}..{:.1}  core {:.0} m away",
        h.root.x,
        h.root.y,
        h.dir.x,
        h.dir.y,
        h.deck,
        h.root_line.iter().copied().fold(f64::MAX, f64::min),
        h.root_line.iter().copied().fold(f64::MIN, f64::max),
        h.root.distance(w.core())
    );
    println!(
        "freighter round {:.0} s, way {} points, {} coasters",
        h.cycle(),
        h.sail.len(),
        h.traffic.len()
    );
    // The map: 2.4 km round the harbour.
    let c = h.to_plan(DVec2::new(100.0, 400.0));
    let half = 1300.0;
    let px = 900.0;
    let k = px / (2.0 * half);
    let to = |p: DVec2| ((p.x - c.x + half) * k, (p.y - c.y + half) * k);
    let mut svg = String::new();
    let _ = writeln!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{px}" height="{px}">"#
    );
    let cell = 10.0;
    let n = (2.0 * half / cell) as i64;
    for j in 0..n {
        for i in 0..n {
            let p = c + DVec2::new(i as f64 * cell - half, j as f64 * cell - half);
            let g = w.ground(p);
            let col = if g < WATER_LEVEL {
                let d = ((-g) * 12.0).min(140.0) as u8;
                format!("rgb({},{},{})", 120 - d / 2, 170 - d / 2, 220 - d / 3)
            } else {
                let v = (g * 3.0).clamp(0.0, 120.0) as u8;
                format!("rgb({},{},{})", 150 + v / 2, 170 + v / 3, 120 + v / 2)
            };
            let (x, y) = to(p);
            let _ = writeln!(
                svg,
                r#"<rect x="{x:.1}" y="{y:.1}" width="{s:.2}" height="{s:.2}" fill="{col}"/>"#,
                s = cell * k + 0.3
            );
        }
    }
    let poly = |svg: &mut String, pts: &[DVec2], style: &str| {
        let d: Vec<String> = pts
            .iter()
            .map(|p| {
                let (x, y) = to(*p);
                format!("{x:.1},{y:.1}")
            })
            .collect();
        let _ = writeln!(svg, r#"<polyline points="{}" {style}/>"#, d.join(" "));
    };
    for r in w.roads_in(c - DVec2::splat(half), c + DVec2::splat(half)) {
        let wpx = (2.0 * r.half * k).max(0.6);
        poly(
            &mut svg,
            &r.pts,
            &format!(r#"fill="none" stroke="dimgray" stroke-width="{wpx:.1}""#),
        );
    }
    let mut mole = h.mole_outline();
    mole.push(mole[0]);
    poly(&mut svg, &mole, r#"fill="silver" stroke="black""#);
    let bw: Vec<DVec2> = BREAKWATER
        .iter()
        .map(|&(x, z)| h.to_plan(DVec2::new(x, z)))
        .collect();
    poly(
        &mut svg,
        &bw,
        &format!(r#"fill="none" stroke="gray" stroke-width="{:.1}""#, 8.0 * k),
    );
    poly(
        &mut svg,
        &h.sail,
        r#"fill="none" stroke="red" stroke-dasharray="4,3""#,
    );
    let mut ships = h.moored();
    ships.push((ShipKind::Freighter, h.freighter_berth()));
    ships.push((ShipKind::Tug, h.tug_berth()));
    for (kind, pose) in ships {
        let d = kind.dims();
        let pts: Vec<DVec2> = [
            (-1.0, -1.0),
            (1.0, -1.0),
            (1.0, 0.8),
            (0.0, 1.0),
            (-1.0, 0.8),
        ]
        .iter()
        .map(|&(x, z)| pose.to_plan(DVec2::new(x * d.beam * 0.5, z * d.len * 0.5)))
        .chain(std::iter::once(
            pose.to_plan(DVec2::new(-d.beam * 0.5, -d.len * 0.5)),
        ))
        .collect();
        poly(&mut svg, &pts, r#"fill="white" stroke="navy""#);
    }
    for l in &w.landmarks {
        let (x, y) = to(l.pos);
        let _ = writeln!(
            svg,
            r#"<circle cx="{x:.1}" cy="{y:.1}" r="{:.1}" fill="orange"/><text x="{x:.1}" y="{y:.1}" font-size="10">{:?}</text>"#,
            (l.plaza * k).max(2.0),
            l.kind
        );
    }
    let _ = writeln!(svg, "</svg>");
    if let Some(dir) = std::path::Path::new(&out).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(&out, svg).expect("write the map");
    println!("map: {out}");
}
