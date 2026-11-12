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
    // A tick is 33 ms; villains may take about a quarter of it (2 ms measured
    // for 300; the old searching took 40 ms).
    let bound = 8_000;
    println!(
        "{villains} villains: enemies {avg} us/tick, worst tick {worst_us} us, {searches} searches, hero at {:?}",
        sim.position(hero)
    );
    assert!(avg < bound, "villains took {avg} us a tick on average");
}
