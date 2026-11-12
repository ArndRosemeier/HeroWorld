//! Counts a town's buildings of halls by scheme, and prints the nearest
//! of each to the start: `halls_count <seed> <people>`.
use glam::DVec2;
use hw_core::Seed;
use hw_gen::urban::world::TownWorld;
use hw_gen::urban::TownParams;
use std::collections::BTreeMap;

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(1);
    let people: f64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(20_000.0);
    let world = TownWorld::generate(Seed(seed), TownParams::sample(Seed(seed), people));
    let (spawn, _) = world.spawn_line();
    let m = DVec2::splat(20_000.0);
    let mut count: BTreeMap<String, (usize, f64, DVec2, u32)> = BTreeMap::new();
    let mut plain: BTreeMap<String, usize> = BTreeMap::new();
    let mut probe = None;
    for q in world.quarters_in(spawn - m, spawn + m) {
        for b in &q.buildings {
            match b.site.layout() {
                Some(l) => {
                    if b.site.usage == hw_gen::building::BuildingUse::Lab {
                        *plain
                            .entry(format!("LAB of halls {:?}", l.scheme))
                            .or_default() += 1;
                    }
                    if !l.problems().is_empty() {
                        println!("PROBLEM {:?} {}: {:?}", l.scheme, b.site.size, l.problems());
                    }
                    let d = b.site.origin.distance(spawn);
                    let e = count.entry(format!("{:?}", l.scheme)).or_insert((
                        0,
                        f64::MAX,
                        DVec2::ZERO,
                        0,
                    ));
                    e.0 += 1;
                    if l.scheme == hw_gen::layout::Scheme::Block && probe.is_none() {
                        probe = Some(b.site.clone());
                    }
                    if d < e.1 {
                        (e.1, e.2, e.3) = (d, b.site.size, b.site.floors);
                    }
                }
                None => {
                    // By use, and by size in steps of 4 m (width x depth).
                    let k = format!(
                        "{:?} {:>2}x{:>2}",
                        b.site.usage,
                        (b.site.size.x / 4.0).floor() * 4.0,
                        (b.site.size.y / 4.0).floor() * 4.0
                    );
                    *plain.entry(k).or_default() += 1
                }
            }
        }
    }
    for (k, (n, d, size, floors)) in count {
        println!("{k}: {n}, nearest {d:.0} m away, {size} x{floors}");
    }
    println!("not of halls:");
    for (k, n) in plain {
        println!("  {k}: {n}");
    }
    if let Some(site) = probe {
        let t = std::time::Instant::now();
        let plan = hw_gen::building::BuildingPlan::generate(&site);
        println!(
            "plan in {:?}: {} floors, {} rooms",
            t.elapsed(),
            plan.floors.len(),
            plan.floors.iter().map(|f| f.rooms.len()).sum::<usize>()
        );
        println!("solids {:?}", plan.solids().map(|s| s.len()));
        println!("shell {:?}", plan.shell_parts(0.05).map(|s| s.len()));
        println!("furniture {}", plan.furniture().len());
        println!("doors {}", plan.door_leaves().len());
        println!(
            "standing {}",
            plan.standing().iter().map(|v| v.len()).sum::<usize>()
        );
    }
}
