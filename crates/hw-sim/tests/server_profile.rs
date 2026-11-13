//! What each part of the server step costs with no, some and many villains
//! (run with `--ignored --nocapture`).
use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_net::PlayerId;
use hw_sim::{GeneratedTown, Sim, TICK_HZ};
use std::collections::BTreeMap;
#[test]
#[ignore = "on demand (balance): prints the server step parts per tick; scripts/check.sh --on-demand"]
fn profile() {
    for groups in [0usize, 60, 250] {
        let town = GeneratedTown::generate(Seed(3), 20_000.0);
        let mut sim = Sim::for_town(Seed(3), &town);
        let start = sim.spawn_point(1);
        sim.spawn_hero(PlayerId(1), start);
        for k in 0..groups {
            let a = k as f64 / groups as f64 * std::f64::consts::TAU;
            let r = 20.0 + (k % 5) as f64 * 6.0;
            sim.add_fight_spot(
                start + DVec3::new(a.cos() * r, 0.0, a.sin() * r),
                DVec2::new(-a.cos(), -a.sin()),
            );
        }
        let mut tot: BTreeMap<&str, u64> = BTreeMap::new();
        let n = TICK_HZ as u64 * 20;
        for _ in 0..n {
            sim.step();
            for (k, v) in sim.take_step_times() {
                *tot.entry(k).or_default() += v;
            }
        }
        println!("== {groups} groups");
        for (k, v) in tot {
            println!("{k}: {} us/tick", v / n);
        }
    }
}
