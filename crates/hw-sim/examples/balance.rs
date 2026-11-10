//! The balance run: the old powers (`--all`: every tree power) against the
//! standard targets.
//! `cargo run --release -p hw-sim --example balance [-- --all]`

use hw_sim::balance;
use hw_sim::powers::{def_of, energy, recharge};

fn main() {
    for level in [8, 12, 16, 20] {
        let (s, h) = balance::baseline(level);
        println!(
            "Strike alone at level {level}: {s:.1} s, {:.0}% health lost",
            h * 100.0
        );
    }
    let all = std::env::args().any(|a| a == "--all");
    let results = if all {
        balance::run_all()
    } else {
        balance::run()
    };
    println!(
        "{:<20} tier  energy  recharge  seconds  hurt  lost  worth  travel",
        "power"
    );
    for r in &results {
        let d = def_of(r.power);
        println!(
            "{:<20} {:>4}  {:>6.0}  {:>8.1}  {:>7.1}  {:>3.0}%  {:>4}  {:>5.2}  {:>6.1}",
            d.name,
            r.tier,
            energy(d),
            recharge(d),
            r.seconds,
            r.hurt * 100.0,
            r.lost,
            r.worth,
            r.travel
        );
    }
    let flags = balance::outliers(&results);
    if flags.is_empty() {
        println!("green: every power within its tier's budget");
    } else {
        for f in &flags {
            println!("OUTLIER: {f}");
        }
        std::process::exit(1);
    }
}
