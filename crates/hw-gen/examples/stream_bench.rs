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
