//! The karma multiplier: the host's setting scales every karma a hero
//! earns on the server, and a world save carries it to a new host.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_net::PlayerId;
use hw_sim::augments::{self, Effect, Grade};
use hw_sim::balance::hero_sheet;
use hw_sim::collision::Obstacles;
use hw_sim::combat::{Fights, Reward};
use hw_sim::save::WorldSave;
use hw_sim::{Sim, GROUND_Y};
use std::sync::Arc;

fn arena(rate: f32) -> Sim {
    let mut sim = Sim::with_solids(Seed(3), Arc::new(Obstacles::flat(GROUND_Y)));
    sim.set_spawn((DVec2::new(35.0, 35.0), DVec2::X));
    sim.set_karma_rate(rate);
    sim
}

/// The lifetime karma of a hero after being paid `karma` by a fight.
fn paid(rate: f32, karma: u32) -> (u32, u32) {
    let mut sim = arena(rate);
    let me = PlayerId(1);
    let hero = sim.spawn_hero_with(me, DVec3::new(0.0, GROUND_Y, 0.0), hero_sheet(3, &[]));
    let before = sim.sheet(me).unwrap();
    sim.world
        .resource_mut::<Fights>()
        .rewards
        .push(Reward { hero, karma });
    // The fights' rewards are paid out once a tick, after the systems that
    // push them; push one and pay it.
    sim.pay_karma();
    let after = sim.sheet(me).unwrap();
    (after.lifetime - before.lifetime, after.karma - before.karma)
}

#[test]
fn earned_karma_is_multiplied_for_level_and_purse() {
    assert_eq!(paid(1.0, 20), (20, 20));
    assert_eq!(paid(2.5, 20), (50, 50));
    assert_eq!(paid(10.0, 20), (200, 200));
    assert_eq!(paid(0.1, 20), (2, 2));
}

#[test]
fn a_trade_in_follows_the_rate() {
    let a = augments::make(Effect::Damage, Grade::Common, 10);
    let base = augments::trade_value(a);
    for rate in [1.0, 4.0, 0.5] {
        let mut sim = arena(rate);
        let me = PlayerId(1);
        sim.spawn_hero_with(me, DVec3::new(0.0, GROUND_Y, 0.0), hero_sheet(3, &[]));
        sim.give_augment(me, a);
        let before = sim.sheet(me).unwrap();
        let got = sim.trade_augment(me, 0).unwrap();
        let after = sim.sheet(me).unwrap();
        let want = hw_sim::karma_rate::scale(base, rate);
        assert_eq!(got, want, "rate {rate}");
        assert_eq!(after.karma - before.karma, want, "rate {rate}");
        assert_eq!(after.lifetime, before.lifetime, "a trade-in is not a level");
    }
}

#[test]
fn the_rate_travels_with_the_world_save() {
    let mut sim = arena(3.5);
    let save = sim.save();
    let text = save.to_text();
    assert!(text.contains("karma_rate 3.5"));
    let back = WorldSave::from_text(&text).unwrap();
    let mut next = arena(1.0);
    next.restore(&back).unwrap();
    assert_eq!(next.karma_rate, 3.5);
    // A save from before the setting plays at the normal rate.
    let old: String = text
        .lines()
        .filter(|l| !l.starts_with("karma_rate"))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_eq!(WorldSave::from_text(&old).unwrap().karma_rate, 1.0);
}
