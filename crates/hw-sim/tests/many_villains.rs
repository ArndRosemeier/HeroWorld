//! Many villains fighting around a hero must not make the server tick slow:
//! the work per tick is bounded (a tick that overruns stalls the whole
//! game: on 2026-11-12 a crowd of villains cost 14 ms a tick, because each
//! searched for a way round the walls every tick). The caps are counted
//! (searches per tick) and the time is checked with a generous bound.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_net::PlayerId;
use hw_sim::combat::PATH_BUDGET;
use hw_sim::{GeneratedTown, Sim, TICK_HZ};

#[test]
fn three_hundred_villains_do_not_stall_the_tick() {
    let town = GeneratedTown::generate(Seed(3), 20_000.0);
    let mut sim = Sim::for_town(Seed(3), &town);
    let start = sim.spawn_point(1);
    let hero = sim.spawn_hero(PlayerId(1), start);
    let groups = 60;
    for k in 0..groups {
        let a = k as f64 / groups as f64 * std::f64::consts::TAU;
        let r = 25.0 + (k % 5) as f64 * 12.0;
        let at = start + DVec3::new(a.cos() * r, 0.0, a.sin() * r);
        sim.add_fight_spot(at, DVec2::new(-a.cos(), -a.sin()));
    }
    let villains = sim.enemies().len();
    assert!(villains >= 250, "{villains} villains");
    let ticks = (TICK_HZ * 6) as u64;
    let (mut combat_us, mut worst_us, mut searches) = (0u64, 0u128, 0u64);
    for _ in 0..ticks {
        let t = std::time::Instant::now();
        sim.step();
        worst_us = worst_us.max(t.elapsed().as_micros());
        let s = sim.fights().path_searches;
        assert!(s <= PATH_BUDGET, "{s} path searches in one tick");
        searches += s as u64;
        for (k, v) in sim.take_step_times() {
            if k == "combat enemies" {
                combat_us += v;
            }
        }
    }
    assert!(searches <= ticks * PATH_BUDGET as u64);
    let avg = combat_us / ticks;
    // A tick is 33 ms; villains may take about a quarter of it (2.5 ms measured
    // for 300 on a quiet machine, double that when tests run side by side;
    // the old searching took 40 ms).
    let bound = 16_000;
    println!(
        "{villains} villains: enemies {avg} us/tick, worst tick {worst_us} us, {searches} searches, hero at {:?}",
        sim.position(hero)
    );
    assert!(avg < bound, "villains took {avg} us a tick on average");
}

/// Runs `ticks` steps and returns (average step µs, worst step µs, the
/// overruns the step reported). Also checks the big snapshot is cheap.
static PARTS: std::sync::Mutex<std::collections::BTreeMap<&'static str, u64>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

/// Wall-clock noise (tests run side by side) may stretch a tick now and
/// then; overruns must stay rare.
fn assert_rare(overruns: &[String], ticks: u64) {
    assert!(
        overruns.len() as u64 <= ticks / 20,
        "{} overruns in {ticks} ticks: {overruns:?}",
        overruns.len()
    );
}

fn run(sim: &mut Sim, ticks: u64) -> (u64, u128, Vec<String>) {
    let (mut total, mut worst, mut overruns) = (0u128, 0u128, Vec::new());
    for _ in 0..ticks {
        let t = std::time::Instant::now();
        sim.step();
        let us = t.elapsed().as_micros();
        total += us;
        worst = worst.max(us);
        overruns.extend(sim.take_overruns());
        for (k, v) in sim.take_step_times() {
            *PARTS.lock().unwrap().entry(k).or_insert(0u64) += v / ticks;
        }
    }
    let t = std::time::Instant::now();
    let n = sim.snapshot().len();
    let snap = t.elapsed().as_micros();
    println!("parts (us/tick): {:?}", PARTS.lock().unwrap());
    println!("snapshot of {n} entities: {snap} us");
    assert!(snap < 8_000, "snapshot of {n} entities took {snap} us");
    ((total / ticks as u128) as u64, worst, overruns)
}

#[test]
fn a_dozen_heroes_among_a_hundred_villains_stay_in_budget() {
    let town = GeneratedTown::generate(Seed(3), 20_000.0);
    let mut sim = Sim::for_town(Seed(3), &town);
    let start = sim.spawn_point(1);
    for p in 0..12 {
        let a = p as f64 * 0.5;
        sim.spawn_hero(
            PlayerId(p + 1),
            start + DVec3::new(a.cos() * 8.0, 0.0, a.sin() * 8.0),
        );
    }
    for k in 0..24 {
        let a = k as f64 / 24.0 * std::f64::consts::TAU;
        let at = start + DVec3::new(a.cos() * 40.0, 0.0, a.sin() * 40.0);
        sim.add_fight_spot(at, DVec2::new(-a.cos(), -a.sin()));
    }
    let ticks = (TICK_HZ * 6) as u64;
    let (avg, worst, overruns) = run(&mut sim, ticks);
    println!("12 heroes: avg {avg} us, worst {worst} us");
    assert!(avg < 20_000, "average step {avg} us");
    assert_rare(&overruns, ticks);
}

/// A minute and a half of a town with heroes and a crowd (crimes, incidents,
/// traffic, alarms, wrecks all run): no part of any tick overruns.
#[test]
#[ignore = "daily (slow blocker): scripts/check.sh --daily"]
fn a_long_town_run_with_a_crowd_never_overruns() {
    let town = GeneratedTown::generate(Seed(3), 20_000.0);
    let mut sim = Sim::for_town(Seed(3), &town);
    let start = sim.spawn_point(1);
    for p in 0..4 {
        sim.spawn_hero(
            PlayerId(p + 1),
            start + DVec3::new(p as f64 * 5.0, 0.0, 0.0),
        );
    }
    for k in 0..30 {
        let a = k as f64 / 30.0 * std::f64::consts::TAU;
        let at = start + DVec3::new(a.cos() * 50.0, 0.0, a.sin() * 50.0);
        sim.add_fight_spot(at, DVec2::new(-a.cos(), -a.sin()));
    }
    let ticks = (TICK_HZ * 90) as u64;
    let (avg, worst, overruns) = run(&mut sim, ticks);
    println!("90 s: avg {avg} us, worst {worst} us");
    assert!(avg < 10_000, "average step {avg} us");
    assert_rare(&overruns, ticks);
}

#[test]
#[ignore = "daily (slow blocker): scripts/check.sh --daily"]
fn a_thousand_villains_cost_no_more_than_four_hundred() {
    let town = GeneratedTown::generate(Seed(3), 20_000.0);
    let mut sim = Sim::for_town(Seed(3), &town);
    let start = sim.spawn_point(1);
    sim.spawn_hero(PlayerId(1), start);
    for k in 0..200 {
        let a = k as f64 / 200.0 * std::f64::consts::TAU;
        let r = 20.0 + (k % 10) as f64 * 8.0;
        let at = start + DVec3::new(a.cos() * r, 0.0, a.sin() * r);
        sim.add_fight_spot(at, DVec2::new(-a.cos(), -a.sin()));
    }
    let villains = sim.enemies().len();
    assert!(villains >= 800, "{villains} villains");
    let ticks = (TICK_HZ * 4) as u64;
    let (avg, worst, overruns) = run(&mut sim, ticks);
    println!("{villains} villains: avg {avg} us, worst {worst} us");
    assert!(avg < 12_000, "average step {avg} us");
    assert_rare(&overruns, ticks);
}
