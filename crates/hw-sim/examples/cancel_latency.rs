//! How quickly a town being planned stops once cancelled (the title throws
//! away a city the player no longer wants):
//! `cargo run --release -p hw-sim --example cancel_latency -- 1000000`
use hw_core::{Cancel, Seed};
use hw_sim::GeneratedTown;
use std::time::{Duration, Instant};

fn main() {
    let pop: f64 = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(1e6);
    let t = Instant::now();
    let _ = GeneratedTown::generate(Seed(1), pop);
    let full = t.elapsed();
    println!("{pop} people: whole plan {full:.2?}");
    let mut worst = Duration::ZERO;
    for step in 1..20 {
        let at = full.mul_f64(step as f64 / 20.0);
        let cancel = Cancel::new();
        let c = cancel.clone();
        let job = std::thread::spawn(move || GeneratedTown::generate_or_stop(Seed(1), pop, &c));
        std::thread::sleep(at);
        let asked = Instant::now();
        cancel.cancel();
        let r = job.join().expect("the plan ran");
        let late = asked.elapsed();
        worst = worst.max(late);
        println!(
            "cancelled at {at:.2?}: stopped {late:.2?} later ({})",
            if r.is_ok() { "had finished" } else { "gave up" }
        );
    }
    println!("worst {worst:.2?}");
}
