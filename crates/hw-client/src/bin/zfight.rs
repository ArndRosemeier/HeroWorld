//! Finds z-fighting in a generated city: faces that lie in the same plane,
//! face the same way and overlap, so they flicker.
//!
//! cargo run -p hw-client --bin zfight -- [seed] [--cut LEVEL] [--all] [--svg FILE]
//!
//! Prints the worst spots (where, which colours, which meshes) and exits
//! with 1 if any differently coloured faces fight. `--all` also lists
//! doubled faces of one colour (wasted, but invisible). `--svg` draws the
//! spots on a map of the city.

use hw_client::zfight::{self, hex, source_name, Clash};
use hw_core::Seed;
use hw_sim::GeneratedCity;
use std::fmt::Write as _;

fn main() {
    let mut seed = 1u64;
    let mut cut = None;
    let mut all = false;
    let mut svg = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--cut" => cut = args.next().and_then(|s| s.parse().ok()),
            "--all" => all = true,
            "--svg" => svg = args.next(),
            s => seed = s.parse().unwrap_or_else(|_| panic!("not a seed: {s}")),
        }
    }
    let city = GeneratedCity::generate(Seed(seed));
    let tris = zfight::city(&city, cut);
    let clashes = zfight::find(&tris, &zfight::city_options());
    let (same, differ): (Vec<Clash>, Vec<Clash>) = clashes.iter().partition(|c| c.same_color);
    println!(
        "seed {seed}{}: {} triangles, {} fighting pairs ({:.1} m²), {} doubled faces of one colour ({:.1} m²)",
        cut.map(|l| format!(", cut at floor {l}")).unwrap_or_default(),
        tris.len(),
        differ.len(),
        differ.iter().map(|c| c.area).sum::<f64>().abs(),
        same.len(),
        same.iter().map(|c| c.area).sum::<f64>().abs(),
    );
    let report = |title: &str, list: &[Clash]| {
        let spots = zfight::spots(list, 10.0);
        if spots.is_empty() {
            return;
        }
        println!("\n{title} ({} spots, worst first):", spots.len());
        for s in spots.iter().take(25) {
            println!(
                "  {:>9.2} m²  at ({:.1}, {:.2}, {:.1})  facing ({:.2}, {:.2}, {:.2})  {} vs {}  [{} / {}]  gap {:.3} m",
                s.area,
                s.at.x,
                s.at.y,
                s.at.z,
                s.normal.x,
                s.normal.y,
                s.normal.z,
                hex(s.colors[0]),
                hex(s.colors[1]),
                source_name(s.sources[0]),
                source_name(s.sources[1]),
                s.gap
            );
        }
    };
    report("Fighting", &differ);
    if all {
        report("Doubled (same colour)", &same);
    }
    if let Some(path) = svg {
        std::fs::write(&path, map(&city, &differ)).expect("write svg");
        println!("\nmap: {path}");
    }
    if !differ.is_empty() {
        std::process::exit(1);
    }
}

/// The city's blocks in grey, spots in magenta sized by area.
fn map(city: &GeneratedCity, clashes: &[Clash]) -> String {
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
    for c in zfight::spots(clashes, 10.0) {
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
