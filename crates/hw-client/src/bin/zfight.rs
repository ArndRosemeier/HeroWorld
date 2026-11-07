//! Finds z-fighting in a generated city: faces that lie in the same plane,
//! face the same way and overlap, so they flicker.
//!
//! cargo run -p hw-client --bin zfight -- [seed] [--cut LEVEL] [--far] [--all] [--svg FILE]
//!
//! Prints the worst spots (where, which colours, which meshes) and exits
//! with 1 if any differently coloured faces fight. Buildings are drawn in
//! near detail unless `--far`. `--all` also lists
//! doubled faces of one colour (wasted, but invisible). `--svg` draws the
//! spots on a map of the city.

use hw_client::buildingmesh::Detail;
use hw_client::zfight::{self, hex, Clash};
use hw_core::Seed;
use hw_sim::GeneratedCity;
use std::fmt::Write as _;

fn main() {
    let mut seed = 1u64;
    let mut cut = None;
    let mut all = false;
    let mut svg = None;
    let mut detail = Detail::Near;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--cut" => cut = args.next().and_then(|s| s.parse().ok()),
            "--all" => all = true,
            "--svg" => svg = args.next(),
            "--far" => detail = Detail::Far,
            s => seed = s.parse().unwrap_or_else(|_| panic!("not a seed: {s}")),
        }
    }
    let city = GeneratedCity::generate(Seed(seed));
    let report = zfight::city(&city, cut, detail).run();
    println!(
        "seed {seed}{}{}: {}",
        cut.map(|l| format!(", cut at floor {l}"))
            .unwrap_or_default(),
        if detail == Detail::Far {
            ", far detail"
        } else {
            ""
        },
        report.summary(25)
    );
    if all {
        let doubled: Vec<Clash> = report.doubled().copied().collect();
        let spots = engine::zfight::spots(&doubled, 10.0);
        println!("\nDoubled (same look, {} spots, worst first):", spots.len());
        for c in spots.iter().take(25) {
            println!(
                "  {:>8.2} m²  at ({:.1}, {:.2}, {:.1})  {}  [{} / {}]",
                c.area,
                c.at.x,
                c.at.y,
                c.at.z,
                hex(c.colors[0]),
                report.source_name(c.sources[0]),
                report.source_name(c.sources[1])
            );
        }
    }
    if let Some(path) = svg {
        std::fs::write(&path, map(&city, &report.spots(10.0))).expect("write svg");
        println!("\nmap: {path}");
    }
    if !report.is_clean() {
        std::process::exit(1);
    }
}

/// The city's blocks in grey, spots in magenta sized by area.
fn map(city: &GeneratedCity, spots: &[Clash]) -> String {
    let r = city.plan.params.radius + 100.0;
    let mut s = String::new();
    let _ = write!(
        s,
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{} {} {} {}" width="1200" height="1200"><rect x="{}" y="{}" width="{}" height="{}" fill="#20242a"/>"##,
        -r,
        -r,
        2.0 * r,
        2.0 * r,
        -r,
        -r,
        2.0 * r,
        2.0 * r
    );
    for b in &city.plan.blocks {
        let pts: Vec<String> = b
            .poly
            .pts
            .iter()
            .map(|p| format!("{:.1},{:.1}", p.x, p.y))
            .collect();
        let _ = write!(
            s,
            r##"<polygon points="{}" fill="#3a4048"/>"##,
            pts.join(" ")
        );
    }
    for c in spots {
        let rad = 3.0 + c.area.sqrt() * 2.0;
        let _ = write!(
            s,
            r##"<circle cx="{:.1}" cy="{:.1}" r="{:.1}" fill="#ff00ff" fill-opacity="0.7"/>"##,
            c.at.x, c.at.z, rad
        );
    }
    s.push_str("</svg>");
    s
}
