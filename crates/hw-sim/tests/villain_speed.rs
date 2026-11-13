//! A villain covers the same distance per second however often it gets its
//! turn: when the server lets far villains act every 2nd or 3rd tick, they
//! walk the ticks they missed, they do not move in slow motion.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_net::PlayerId;
use hw_net::PowerId;
use hw_sim::balance::hero_sheet;
use hw_sim::collision::Obstacles;
use hw_sim::combat::EnemyKind;
use hw_sim::{Sim, GROUND_Y, TICK_HZ};
use std::sync::Arc;

/// Metres the villains walk in 1.5 seconds, towards a hero 14 m away, when
/// each may act only every `stride`th tick.
fn walked(kind: EnemyKind, stride: u64) -> f64 {
    let mut sim = Sim::with_solids(Seed(4), Arc::new(Obstacles::flat(GROUND_Y)));
    sim.set_spawn((DVec2::new(300.0, 300.0), DVec2::X));
    sim.spawn_hero_with(
        PlayerId(1),
        DVec3::new(0.0, GROUND_Y, 0.0),
        hero_sheet(10, &[PowerId::STRIKE]),
    );
    sim.step();
    sim.add_fight_spot_of(
        DVec3::new(14.0, GROUND_Y, 0.0),
        DVec2::new(-1.0, 0.0),
        kind,
        3,
    );
    sim.set_think_stride(stride);
    // Settle (they notice, line up), then measure.
    for _ in 0..TICK_HZ / 4 {
        sim.step();
    }
    let before = sim.enemies();
    for _ in 0..TICK_HZ * 3 / 2 {
        sim.step();
    }
    let after = sim.enemies();
    assert_eq!(before.len(), after.len());
    before
        .iter()
        .zip(&after)
        .map(|(b, a)| (a.2 - b.2).length())
        .sum::<f64>()
        / before.len() as f64
}

#[test]
fn a_villain_walks_as_far_on_every_stride() {
    for kind in [EnemyKind::Brute] {
        let base = walked(kind, 1);
        assert!(base > 3.0, "{kind:?} walked only {base:.1} m");
        for stride in [2, 3, 4] {
            let d = walked(kind, stride);
            assert!(
                (d - base).abs() < base * 0.15,
                "{kind:?}: {d:.1} m on stride {stride}, {base:.1} m on stride 1"
            );
        }
    }
}
