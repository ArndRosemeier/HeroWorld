//! How big the town's building plots come out, by use: for tuning plots
//! against what the floor planners need.
use hw_core::Seed;
use hw_gen::layout::office::MIN_SIZE;
use hw_gen::urban::params::TownParams;
use hw_gen::urban::plots::plan_buildings;
use hw_gen::urban::streets::plan_piece;
use hw_gen::urban::Town;
use std::collections::BTreeMap;

fn main() {
    let pop: f64 = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(40_000.0);
    for s in 0..3u64 {
        let town = Town::generate(Seed(s), TownParams::sample(Seed(s), pop));
        // use -> (count, fits the office planner, area, floors)
        let mut by: BTreeMap<String, (u32, u32, f64, u32)> = BTreeMap::new();
        for (i, pc) in town.pieces.iter().enumerate() {
            let streets = plan_piece(Seed(s), &town, pc);
            for b in plan_buildings(Seed(s), &town, i, pc, &streets) {
                let e = by
                    .entry(format!("{:?}/{:?}", b.district, b.usage))
                    .or_default();
                e.0 += 1;
                e.1 += (b.size.x >= MIN_SIZE.x && b.size.y >= MIN_SIZE.y) as u32;
                e.2 += b.size.x * b.size.y;
                e.3 += b.floors;
            }
        }
        println!("seed {s}, {pop} people");
        for (k, (n, fit, area, floors)) in by {
            println!(
                "  {k:32} {n:5} buildings, {fit:5} fit, mean {:5.0} m2, {:4.1} floors",
                area / n as f64,
                floors as f64 / n as f64
            );
        }
    }
}
