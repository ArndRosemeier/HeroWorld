//! Where the Doctor's story plays in a town: each chapter's place, the
//! contact at Hero Hall and the statue's plinth (for `HW_AT` screenshots).
//! `cargo run --release -p hw-sim --example story_places -- <seed> <people>`

use hw_core::Seed;
use hw_sim::story::CHAPTERS;
use hw_sim::{GeneratedTown, Sim};

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(1);
    let people: f64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(40_000.0);
    let town = GeneratedTown::generate(Seed(seed), people);
    let sim = Sim::for_town(Seed(seed), &town);
    if !sim.story.available() {
        println!("seed {seed}, {people} people: no story here (no villain quarter or Hero Hall)");
        return;
    }
    let hall = sim.story.hall().expect("a hall");
    println!("contact at {:.1},{:.1},{:.1}", hall.x, hall.y, hall.z);
    for (k, c) in CHAPTERS.iter().enumerate() {
        let (at, f) = sim.story.place(k).expect("placed");
        println!(
            "{} {:<22} level {:>2}  at {:.1},{:.1},{:.1}  facing {:.2},{:.2}",
            k + 1,
            c.title,
            c.level,
            at.x,
            at.y,
            at.z,
            f.x,
            f.y
        );
    }
}
