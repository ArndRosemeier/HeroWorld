//! How long the street traffic takes to plan and to drive, for a town of
//! `HW_POP` people (default 40 000): `cargo run --release -p hw-sim --example traffic_cost`.

use hw_core::Seed;
use hw_sim::traffic::{Traffic, STEP};
use hw_sim::GeneratedTown;
use std::time::Instant;

fn main() {
    let pop: f64 = std::env::var("HW_POP")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(40_000.0);
    let town = GeneratedTown::generate(Seed(1), pop);
    let t = Instant::now();
    let mut traffic = Traffic::new(town.area.clone());
    let (cars, fliers) = traffic.count();
    println!(
        "{cars} cars, {fliers} fliers, planned in {:.2} s",
        t.elapsed().as_secs_f64()
    );
    let steps = 600;
    let t = Instant::now();
    traffic.advance_to(steps as f64 * STEP);
    let ms = t.elapsed().as_secs_f64() * 1000.0 / steps as f64;
    println!(
        "{ms:.2} ms per step ({:.1} ms per second of game)",
        ms / STEP
    );
}
