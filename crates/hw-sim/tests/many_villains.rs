//! Many villains fighting around a hero must not make the server tick slow:
//! the work per tick is bounded (a tick that overruns stalls the whole
//! game: on 2026-11-12 a crowd of villains cost 14 ms a tick, because each
//! searched for a way round the walls every tick).
//!
//! The blockers count work (path searches and acting villains per tick,
//! traffic blockers), which is the same on every machine and under any load.
//! Wall-clock bounds live in the daily tests: tests that run side by side
//! stretch ticks at random.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_net::PlayerId;
use hw_sim::combat::{ACT_CAP, PATH_BUDGET, THINK_CAP};
use hw_sim::{GeneratedTown, Sim, TICK_HZ};

/// A town with one hero at the start and `groups` groups of villains round
/// it (`radius` and `ring` metres out), plus `heroes - 1` more heroes.
fn crowd(groups: usize, heroes: u16, radius: f64, ring: f64) -> (Sim, usize) {
    let town = GeneratedTown::generate(Seed(3), 20_000.0);
    let mut sim = Sim::for_town(Seed(3), &town);
    let start = sim.spawn_point(1);
    for p in 0..heroes {
        let a = p as f64 * 0.5;
        sim.spawn_hero(
            PlayerId(p + 1),
            start + DVec3::new(a.cos() * 8.0, 0.0, a.sin() * 8.0),
        );
    }
    for k in 0..groups {
        let a = k as f64 / groups as f64 * std::f64::consts::TAU;
        let r = radius + (k % 5) as f64 * ring;
        let at = start + DVec3::new(a.cos() * r, 0.0, a.sin() * r);
        sim.add_fight_spot(at, DVec2::new(-a.cos(), -a.sin()));
    }
    let n = sim.enemies().len();
    (sim, n)
}

/// What a run counted.
struct Run {
    avg_us: u64,
    worst_us: u128,
    searches: u64,
    most_acting: u32,
    overruns: usize,
}

/// Steps `ticks` times, asserting the work counters every tick.
fn run(sim: &mut Sim, ticks: u64) -> Run {
    let mut r = Run {
        avg_us: 0,
        worst_us: 0,
        searches: 0,
        most_acting: 0,
        overruns: 0,
    };
    let mut total = 0u128;
    for _ in 0..ticks {
        let t = std::time::Instant::now();
        sim.step();
        let us = t.elapsed().as_micros();
        total += us;
        r.worst_us = r.worst_us.max(us);
        let f = sim.fights();
        assert!(
            f.path_searches <= PATH_BUDGET,
            "{} path searches in one tick",
            f.path_searches
        );
        assert!(
            u64::from(f.villains_acted) <= ACT_CAP,
            "{} villains acted in one tick",
            f.villains_acted
        );
        r.searches += u64::from(f.path_searches);
        r.most_acting = r.most_acting.max(f.villains_acted);
        r.overruns += sim.take_overruns().len();
        sim.take_step_times();
    }
    r.avg_us = (total / u128::from(ticks)) as u64;
    assert!(r.searches <= ticks * u64::from(PATH_BUDGET));
    r
}

#[test]
fn three_hundred_villains_search_and_act_within_their_caps() {
    let (mut sim, villains) = crowd(60, 1, 25.0, 12.0);
    assert!(villains >= 250, "{villains} villains");
    let r = run(&mut sim, u64::from(TICK_HZ) * 6);
    // Under the cap everyone acts every tick.
    assert_eq!(u64::from(r.most_acting), villains as u64);
    assert!(r.searches > 0, "the crowd searched for no way at all");
}

#[test]
fn a_crowd_over_the_cap_acts_in_turns() {
    let (mut sim, villains) = crowd(100, 1, 20.0, 8.0);
    assert!(villains as u64 > THINK_CAP, "{villains} villains");
    let r = run(&mut sim, u64::from(TICK_HZ) * 4);
    let [full, reduced, _] = sim.fights().tier_counts;
    assert!(full as u64 <= THINK_CAP && reduced > 0, "{full} {reduced}");
    assert!(r.most_acting > 0);
    assert!(
        u64::from(r.most_acting) < villains as u64,
        "all {villains} acted in one tick"
    );
}

/// A thousand villains round one hero: the first ring is full, the second
/// takes turns, the rest wait, and no tick does more than the cap.
#[test]
fn a_thousand_villains_fill_the_rings_and_no_more() {
    let (mut sim, villains) = crowd(250, 1, 20.0, 6.0);
    assert!(villains >= 900, "{villains} villains");
    let r = run(&mut sim, u64::from(TICK_HZ) * 3);
    let [full, reduced, dormant] = sim.fights().tier_counts;
    assert_eq!(full + reduced + dormant, villains);
    assert!(full as u64 <= THINK_CAP);
    assert!(u64::from(r.most_acting) <= ACT_CAP);
}

#[test]
fn a_dozen_heroes_among_villains_keep_to_the_caps() {
    let (mut sim, villains) = crowd(24, 12, 40.0, 0.0);
    assert!(villains >= 100, "{villains} villains");
    run(&mut sim, u64::from(TICK_HZ) * 6);
    assert!(sim.snapshot().len() >= villains + 12);
}

/// The same checks against the clock, which only a quiet machine can keep.
#[test]
#[ignore = "daily (slow blocker): scripts/check.sh --daily"]
fn crowds_take_little_time_on_a_quiet_machine() {
    for (groups, heroes, radius, ring, avg_bound) in [
        (60, 1, 25.0, 12.0, 8_000u64),
        (24, 12, 40.0, 0.0, 10_000),
        (200, 1, 20.0, 8.0, 12_000),
    ] {
        let (mut sim, villains) = crowd(groups, heroes, radius, ring);
        let r = run(&mut sim, u64::from(TICK_HZ) * 6);
        println!(
            "{villains} villains, {heroes} heroes: avg {} us, worst {} us, {} overruns",
            r.avg_us, r.worst_us, r.overruns
        );
        assert!(r.avg_us < avg_bound, "average step {} us", r.avg_us);
        let t = std::time::Instant::now();
        let n = sim.snapshot().len();
        assert!(t.elapsed().as_micros() < 8_000, "snapshot of {n}");
        assert!(
            r.overruns <= 6 * TICK_HZ as usize / 20,
            "{} overruns",
            r.overruns
        );
    }
}

/// A minute and a half of a town with heroes and a crowd (crimes, incidents,
/// traffic, alarms, wrecks all run): the step stays cheap.
#[test]
#[ignore = "daily (slow blocker): scripts/check.sh --daily"]
fn a_long_town_run_with_a_crowd_stays_cheap() {
    let (mut sim, _) = crowd(30, 4, 50.0, 0.0);
    let r = run(&mut sim, u64::from(TICK_HZ) * 90);
    println!(
        "90 s: avg {} us, worst {} us, {} overruns",
        r.avg_us, r.worst_us, r.overruns
    );
    assert!(r.avg_us < 10_000, "average step {} us", r.avg_us);
    assert!(
        r.overruns <= 90 * TICK_HZ as usize / 20,
        "{} overruns",
        r.overruns
    );
}
