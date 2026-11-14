//! A town's rivers from their heads to the town, pool by pool: where each
//! lies, how wide it is, the level of its water and the land beside it.
//!
//! ```
//! cargo run --release -p hw-gen --example river_profile -- <seed> [people]
//! ```

use hw_core::Seed;
use hw_gen::planet::{choose_home, PlanetMap, PlanetPatch};
use hw_gen::urban::{params::TownParams, world::TownWorld};

fn main() {
    let mut args = std::env::args().skip(1);
    let seed = Seed(args.next().and_then(|s| s.parse().ok()).unwrap_or(1));
    let people: f64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(20_000.0);
    // The same planet map the game solves (`hw_sim::home_patch`).
    let map = PlanetMap::build(seed, 192);
    let patch = std::sync::Arc::new(PlanetPatch::new(&map, choose_home(&map)));
    let params = TownParams::on_planet(seed, people, &patch);
    let town =
        TownWorld::generate_on_or_stop(seed, params, Some(patch.clone()), &hw_core::Cancel::new())
            .expect("not cancelled");
    let Some(outer) = town.outer_rivers() else {
        println!("no rivers");
        return;
    };
    for (ri, (r, levels)) in outer.rivers.iter().zip(&outer.levels).enumerate() {
        println!(
            "river {ri}: {} points, head at {:.0} m",
            r.pts.len(),
            levels[0]
        );
        println!(
            "{:>7} {:>9} {:>9} {:>7} {:>8} {:>8} {:>8}",
            "km", "x", "z", "width", "water", "bed", "land"
        );
        let mut s = 0.0;
        for k in 0..r.pts.len() {
            if k > 0 {
                s += r.pts[k].distance(r.pts[k - 1]);
            }
            let fall = k + 1 < r.pts.len() && levels[k] > levels[k + 1];
            println!(
                "{:7.2} {:9.0} {:9.0} {:7.0} {:8.2} {:8.2} {:8.2}{}",
                s / 1000.0,
                r.pts[k].x,
                r.pts[k].y,
                r.widths[k],
                levels[k],
                town.ground(r.pts[k]),
                patch.ground(r.pts[k]),
                if fall {
                    format!("   fall {:.1} m", levels[k] - levels[k + 1])
                } else {
                    String::new()
                }
            );
        }
    }
}
