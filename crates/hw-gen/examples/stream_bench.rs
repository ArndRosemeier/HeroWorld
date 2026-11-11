//! How long a town takes to plan, and how long the detail around the
//! heroes' start takes once it is asked for (it is planned only there).
//!
//! `cargo run --release -p hw-gen --example stream_bench -- 20000 2000000 15000000`

use glam::DVec2;
use hw_core::Seed;
use hw_gen::urban::{world::TownWorld, TownParams};
use std::time::{Duration, Instant};

fn main() {
    let pops: Vec<f64> = std::env::args()
        .skip(1)
        .filter_map(|a| a.parse().ok())
        .collect();
    for pop in pops {
        let seed = Seed(1);
        let t = Instant::now();
        let w = TownWorld::generate(seed, TownParams::sample(seed, pop));
        let plan = t.elapsed();
        let (start, _) = w.spawn_line();
        let t = Instant::now();
        let r = DVec2::splat(1300.0);
        let units = w.units_in(start - r, start + r);
        let (mut buildings, mut props, mut worst) = (0, 0, Duration::ZERO);
        for u in &units {
            let t1 = Instant::now();
            let q = w.quarter(*u);
            worst = worst.max(t1.elapsed());
            buildings += q.buildings.len();
            props += q.props.len();
        }
        let detail = t.elapsed();
        // The skyline out to 6 km, in 768 m tiles, as the client draws it.
        let t = Instant::now();
        let (mut far, mut tiles, mut slowest) = (0, 0, Duration::ZERO);
        let tile = 768.0;
        let n = (6000.0f64 / tile).ceil() as i64;
        for j in -n..n {
            for i in -n..n {
                let lo = start + DVec2::new(i as f64, j as f64) * tile;
                let hi = lo + DVec2::splat(tile);
                if lo
                    .max(-DVec2::splat(w.half()))
                    .cmpgt(hi.min(DVec2::splat(w.half())))
                    .any()
                {
                    continue;
                }
                let t1 = Instant::now();
                far += w.skyline(lo, hi).len();
                slowest = slowest.max(t1.elapsed());
                tiles += 1;
            }
        }
        println!(
            "  skyline 6 km round the start: {tiles} tiles, {far} buildings in {:.2?} (slowest tile {slowest:.2?}), quarters kept {}",
            t.elapsed(),
            w.quarters_kept()
        );
        let t = Instant::now();
        let lifts = w.underground().entrances.len();
        let under = t.elapsed();
        println!(
            "{pop} people: town planned in {plan:.2?}; 1.3 km round the start: {} units in \
             {detail:.2?} (slowest {worst:.2?}), {buildings} buildings, {props} props; \
             underground {under:.2?}, {lifts} lifts",
            units.len()
        );
    }
}
