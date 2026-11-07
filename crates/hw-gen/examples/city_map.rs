//! Draws a city plan as an SVG map, to look at the generator's output.
//! Usage: cargo run -p hw-gen --example city_map -- [seed] [out.svg] [radius]
//!
//! `radius` (m) sets the size: about 100 for a hamlet, 1300 (the default)
//! for a big city, several thousand for a megalopolis.

use hw_core::Seed;
use hw_gen::building::BuildingUse;
use hw_gen::city::{BlockUse, CityParams, CityPlan, DistrictKind, StreetClass};
use std::fmt::Write;

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let out = args
        .next()
        .unwrap_or_else(|| "screenshots/city_map.svg".into());
    let radius: f64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1300.0);
    let t = std::time::Instant::now();
    let plan = CityPlan::generate(Seed(seed), &CityParams::with_radius(radius));
    println!("planned in {:.2} s", t.elapsed().as_secs_f64());
    stats(&plan);
    let r = plan.params.radius * 1.2;
    let mut svg = String::new();
    let _ = writeln!(
        svg,
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{} {} {} {}" width="1600" height="1600"><rect x="{}" y="{}" width="{}" height="{}" fill="#3f6f9a"/>"##,
        -r,
        -r,
        2.0 * r,
        2.0 * r,
        -r,
        -r,
        2.0 * r,
        2.0 * r
    );
    let poly = |pts: &[glam::DVec2]| {
        pts.iter()
            .map(|p| format!("{:.1},{:.1}", p.x, p.y))
            .collect::<Vec<_>>()
            .join(" ")
    };
    // Land over the water, then promenades and beaches.
    for l in &plan.land {
        let _ = writeln!(
            svg,
            r##"<polygon points="{}" fill="#5d7a4e"/>"##,
            poly(&l.pts)
        );
    }
    for (strip, street) in &plan.waterfront {
        let fill = match plan.streets[*street as usize].class {
            StreetClass::Shore => "#e3d29a",
            _ => "#cfc9bb",
        };
        let _ = writeln!(
            svg,
            r##"<polygon points="{}" fill="{fill}"/>"##,
            poly(&strip.pts)
        );
    }
    for b in &plan.bridges {
        let s = &plan.streets[b.street as usize];
        let _ = writeln!(
            svg,
            r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#8a6d4a" stroke-width="{:.1}"/>"##,
            b.a.x,
            b.a.y,
            b.b.x,
            b.b.y,
            s.width + 2.0 * s.sidewalk + 3.0
        );
    }
    for s in plan
        .streets
        .iter()
        .filter(|s| !s.class.is_water() && s.a != s.b)
    {
        let _ = writeln!(
            svg,
            r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#cfc9bb" stroke-width="{:.1}" stroke-linecap="round"/>"##,
            s.a.x,
            s.a.y,
            s.b.x,
            s.b.y,
            s.width + 2.0 * s.sidewalk
        );
    }
    for s in plan
        .streets
        .iter()
        .filter(|s| !s.class.is_water() && s.a != s.b)
    {
        let _ = writeln!(
            svg,
            r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#55565c" stroke-width="{:.1}"/>"##,
            s.a.x, s.a.y, s.b.x, s.b.y, s.width
        );
    }
    for b in &plan.blocks {
        let kind = plan.districts[b.district].kind;
        let fill = match (b.usage, kind) {
            (BlockUse::Park, _) => "#6f9a52",
            (BlockUse::Plaza, _) => "#d9b9a6",
            (_, DistrictKind::Civic) => "#e9e2cf",
            (_, DistrictKind::Commercial) => "#efd9b8",
            (_, DistrictKind::Residential) => "#cfe0b4",
            (_, DistrictKind::AetherWorks) => "#c9d3dc",
            (_, DistrictKind::Expo) => "#f0e3a8",
            (_, DistrictKind::Dystopia) => "#8f948a",
        };
        let _ = writeln!(
            svg,
            r##"<polygon points="{}" fill="{}"/>"##,
            poly(&b.poly.pts),
            fill
        );
    }
    for l in &plan.lots {
        let _ = writeln!(
            svg,
            r##"<polygon points="{}" fill="none" stroke="#00000022" stroke-width="0.6"/>"##,
            poly(&l.poly.pts)
        );
    }
    for (_, s) in &plan.buildings {
        let h = s.height();
        let shade = (230.0 - h.min(140.0)) as u8;
        let fill = match s.usage {
            BuildingUse::Lab => "#4b8f3a".to_string(),
            BuildingUse::Tenement => "#5a5a55".to_string(),
            BuildingUse::Pavilion => "#e8b04a".to_string(),
            BuildingUse::Works => "#7d8fa3".to_string(),
            _ => format!(
                "rgb({shade},{},{})",
                shade.saturating_sub(10),
                shade.saturating_sub(30)
            ),
        };
        let _ = writeln!(
            svg,
            r##"<polygon points="{}" fill="{}" stroke="#222" stroke-width="0.5"/>"##,
            poly(&s.corners()),
            fill
        );
    }
    for d in &plan.districts {
        let _ = writeln!(
            svg,
            r##"<text x="{:.0}" y="{:.0}" font-size="{:.0}" font-family="sans-serif" text-anchor="middle" fill="#a0102a" stroke="#fff" stroke-width="{:.1}">{:?}</text>"##,
            d.center.x,
            d.center.y,
            r / 28.0,
            r / 900.0,
            d.kind
        );
    }
    svg.push_str("</svg>\n");
    if let Some(dir) = std::path::Path::new(&out).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(&out, svg).expect("write svg");
    println!(
        "seed {seed}: {} districts, {} streets, {} blocks, {} lots, {} buildings -> {out}",
        plan.districts.len(),
        plan.streets.len(),
        plan.blocks.len(),
        plan.lots.len(),
        plan.buildings.len()
    );
}

#[allow(dead_code)]
fn stats(plan: &CityPlan) {
    use std::collections::BTreeMap;
    let mut m: BTreeMap<String, (usize, usize, usize, usize)> = BTreeMap::new();
    for b in &plan.blocks {
        let e = m
            .entry(format!("{:?}", plan.districts[b.district].kind))
            .or_default();
        match b.usage {
            BlockUse::Lots => e.0 += 1,
            _ => e.1 += 1,
        }
    }
    for l in &plan.lots {
        m.entry(format!(
            "{:?}",
            plan.districts[plan.blocks[l.block].district].kind
        ))
        .or_default()
        .2 += 1;
    }
    for (l, _) in &plan.buildings {
        m.entry(format!(
            "{:?}",
            plan.districts[plan.blocks[plan.lots[*l].block].district].kind
        ))
        .or_default()
        .3 += 1;
    }
    for (k, v) in m {
        println!(
            "{k}: lot blocks {}, plaza/park {}, lots {}, buildings {}",
            v.0, v.1, v.2, v.3
        );
    }
}
