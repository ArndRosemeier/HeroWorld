//! Where a town's graveyards lie: `cargo run --release -p hw-gen --example graveyards -- <seed> <people>`.
use hw_core::Seed;
use hw_gen::urban::world::TownWorld;
use hw_gen::urban::TownParams;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seed: u64 = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(1);
    let pop: f64 = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(20_000.0);
    let w = TownWorld::generate(Seed(seed), TownParams::sample(Seed(seed), pop));
    println!(
        "{} pieces, {} graveyards",
        w.town.pieces.len(),
        w.town.graveyards.len()
    );
    for g in &w.town.graveyards {
        let unit = w.unit_at(g.at);
        let s = w.streets(unit);
        let yard = s.as_ref().and_then(|s| s.plan.graveyard.clone());
        if std::env::var_os("GY_DEBUG").is_some() {
            if let Some(s) = &s {
                for (k, b) in s.plan.blocks.iter().enumerate() {
                    let a = hw_gen::urban::graph::signed_area(b).abs();
                    let c = hw_gen::urban::inner_point(b).unwrap_or_default();
                    println!(
                        "  block {k}: {a:.0} m² at {:.0},{:.0} holds site {}",
                        c.x,
                        c.y,
                        hw_gen::urban::contains(b, g.at)
                    );
                }
                println!("  streets {}", s.plan.streets.len());
                if let Some(r) = hw_gen::urban::graveyard::reserve(
                    g,
                    &s.piece.face.pts,
                    s.piece.face.area,
                    w.town.params.population,
                ) {
                    println!("  reserve {:?}", r);
                    if let Some(b) = s
                        .plan
                        .blocks
                        .iter()
                        .find(|b| hw_gen::urban::contains(b, g.at))
                    {
                        println!("  block {:?}", b);
                    }
                    if let Some(y) = &s.plan.graveyard {
                        println!("  outline {:?} block {}", y.outline, y.block);
                    }
                    for st in &s.plan.streets {
                        let n = st
                            .iter()
                            .filter(|p| hw_gen::urban::contains(&r, **p))
                            .count();
                        if n > 0 {
                            println!("   street with {n}/{} points inside: {:?}", st.len(), st);
                        }
                    }
                }
            }
        }
        if let Some(y) = &yard {
            let q = w.quarter(unit);
            let mut counts = std::collections::BTreeMap::<String, usize>::new();
            for p in &q.props {
                if y.contains(p.pos) || y.distance(p.pos) < 4.0 {
                    let name = format!("{:?}", p.kind);
                    let name = name.split('(').next().unwrap_or("").to_string();
                    *counts.entry(name).or_default() += 1;
                }
            }
            println!("  props: {counts:?}");
            if std::env::var_os("GY_EDGES").is_some() {
                let n = y.outline.len();
                for k in 0..n {
                    let (a, b) = (y.outline[k], y.outline[(k + 1) % n]);
                    let rails = q
                        .props
                        .iter()
                        .filter(|p| {
                            p.kind == hw_gen::urban::props::PropKind::GraveRail
                                && hw_gen::water::segment_distance(p.pos, a, b) < 12.0
                        })
                        .count();
                    println!(
                        "   edge {k}{}: {:.0} m, {rails} rails",
                        if k == y.gate_edge { " (gate)" } else { "" },
                        a.distance(b)
                    );
                }
            }
        }
        match yard {
            Some(y) => println!(
                "{:?} at ({:.0}, {:.0}) piece {} era {:?}: style {:?}, {:.0} m², {} corners, district {:.0} m²",
                g.kind,
                g.at.x,
                g.at.y,
                g.piece,
                w.town.pieces[g.piece].era,
                s.as_ref().unwrap().plan.style,
                hw_gen::urban::graph::signed_area(&y.outline).abs(),
                y.outline.len(),
                s.as_ref().unwrap().piece.face.area
            ),
            None => println!("{:?} at {:?}: NO BLOCK (unit {:?})", g.kind, g.at, unit.kind()),
        }
    }
}

#[allow(dead_code)]
fn debug() {}
