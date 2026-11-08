//! The balance run: every power against the standard targets.
//! `cargo run --release -p hw-sim --example balance`

use hw_sim::balance;
use hw_sim::combat::power_spec;

fn main() {
    let (s, h) = balance::baseline();
    println!("Strike alone: {s:.1} s, {:.0}% health lost", h * 100.0);
    let results = balance::run();
    println!("{:<16} tier  seconds  hurt  lost  worth  travel", "power");
    for r in &results {
        println!(
            "{:<16} {:>4}  {:>7.1}  {:>3.0}%  {:>4}  {:>5.2}  {:>6.1}",
            power_spec(r.power).name,
            r.tier,
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
