//! The mad scientist's groups have roles: a shield bearer whose shield
//! stops blows from afar, a grenadier whose bombs burst where a hero stood
//! (step out of the ring), and a lab orderly who patches up the others.

use glam::{DVec2, DVec3};
use hw_core::{NetId, Seed};
use hw_net::{FxKind, InputState, PlayerId, Power};
use hw_sim::balance::hero_sheet;
use hw_sim::collision::{Body, Obstacles};
use hw_sim::combat::{with_roles, EnemyKind, BOMB_SECONDS, ORDERLY_EVERY};
use hw_sim::{Sim, GROUND_Y, TICK_HZ};
use std::sync::Arc;

fn ground(x: f64, z: f64) -> DVec3 {
    DVec3::new(x, GROUND_Y, z)
}

/// A level 10 hero at the origin with Strike and Energy Bolt, and `size`
/// villains of `kind` `at` metres down the street.
fn street(kind: EnemyKind, size: u32, at: f64) -> (Sim, NetId, Vec<NetId>) {
    let mut sim = Sim::with_solids(Seed(4), Arc::new(Obstacles::flat(GROUND_Y)));
    sim.set_spawn((DVec2::new(60.0, 60.0), DVec2::X));
    let hero = sim.spawn_hero_with(
        PlayerId(1),
        ground(0.0, 0.0),
        hero_sheet(10, &[Power::Strike, Power::Bolt]),
    );
    sim.step();
    sim.add_fight_spot_of(ground(at, 0.0), DVec2::new(-1.0, 0.0), kind, size);
    sim.step();
    let foes: Vec<NetId> = sim.enemies().iter().map(|e| e.0).collect();
    (sim, hero, foes)
}

fn put(sim: &mut Sim, id: NetId, at: DVec3) {
    sim.set_body(
        id,
        Body {
            pos: at,
            vel: DVec3::ZERO,
            grounded: true,
        },
    );
}

/// Steps `ticks` ticks with `input` on the first; returns every event.
fn run(sim: &mut Sim, input: InputState, ticks: u64) -> Vec<hw_net::Fx> {
    let mut seen = Vec::new();
    for k in 0..ticks {
        let i = if k == 0 {
            input
        } else {
            InputState {
                target: input.target,
                aim: input.aim,
                ..Default::default()
            }
        };
        sim.set_input(PlayerId(1), i);
        sim.step();
        seen.extend(sim.fx().iter().copied());
    }
    seen
}

fn power(p: Power, target: NetId) -> InputState {
    InputState {
        power: p.code(),
        target: Some(target),
        aim: [1.0, 0.0, 0.0],
        ..Default::default()
    }
}

#[test]
fn the_doctors_groups_have_roles_and_other_crews_do_not() {
    let base = |n: usize| -> Vec<(EnemyKind, u8)> {
        (0..n)
            .map(|k| {
                let kind = if k % 5 == 1 || k % 5 == 3 {
                    EnemyKind::Technician
                } else {
                    EnemyKind::Brute
                };
                (kind, 5)
            })
            .collect()
    };
    let count = |g: &[(EnemyKind, u8)], k: EnemyKind| g.iter().filter(|x| x.0 == k).count();
    for seed in 0..20 {
        let mut five = base(5);
        with_roles(Seed(seed), &mut five);
        assert_eq!(count(&five, EnemyKind::Bulwark), 1);
        assert_eq!(
            count(&five, EnemyKind::Grenadier) + count(&five, EnemyKind::Orderly),
            1,
            "{five:?}"
        );
        // Still someone to shoot and someone to brawl.
        assert!(count(&five, EnemyKind::Brute) >= 1);
        assert!(count(&five, EnemyKind::Technician) >= 1);
        let mut seven = base(7);
        with_roles(Seed(seed), &mut seven);
        assert_eq!(count(&seven, EnemyKind::Grenadier), 1);
        assert_eq!(count(&seven, EnemyKind::Orderly), 1);
        let mut three = base(3);
        with_roles(Seed(seed), &mut three);
        assert_eq!(three, base(3), "small groups stay plain");
    }
}

#[test]
fn the_shield_stops_bolts_from_afar_but_not_a_punch() {
    let (mut sim, hero, foes) = street(EnemyKind::Bulwark, 1, 10.0);
    let wall = foes[0];
    // Hold everyone in place: the first bolt makes the hero its foe.
    let mut blocked = 0;
    let mut bolts = 0;
    for _ in 0..3 {
        put(&mut sim, hero, ground(0.0, 0.0));
        put(&mut sim, wall, ground(10.0, 0.0));
        let fx = run(&mut sim, power(Power::Bolt, wall), TICK_HZ as u64 * 2);
        bolts += fx
            .iter()
            .filter(|e| e.kind == FxKind::Bolt && e.to == wall)
            .count();
        blocked += fx
            .iter()
            .filter(|e| e.kind == FxKind::Block && e.to == wall && e.from == hero)
            .count();
    }
    assert!(bolts >= 3, "bolts {bolts}");
    assert!(blocked >= 2, "the shield took {blocked} of {bolts}");
    // Up close a punch goes round it.
    put(&mut sim, wall, ground(1.5, 0.0));
    put(&mut sim, hero, ground(0.0, 0.0));
    let fx = run(&mut sim, power(Power::Strike, wall), 4);
    assert!(fx.iter().any(|e| e.kind == FxKind::Strike && e.to == wall));
    assert!(!fx.iter().any(|e| e.kind == FxKind::Block));
}

/// A grenadier lobs at the hero; the hero `steps` metres aside once the
/// bomb is in the air. Returns (bombs that burst, damage taken from them).
fn bombed(steps: f64) -> (usize, u32) {
    let (mut sim, hero, foes) = street(EnemyKind::Grenadier, 1, 12.0);
    let g = foes[0];
    let mut lobbed = None;
    let mut bursts = 0;
    let mut hurt = 0;
    for _ in 0..TICK_HZ as u64 * 8 {
        put(&mut sim, g, ground(12.0, 0.0));
        let at = if lobbed.is_some() {
            ground(0.0, steps)
        } else {
            ground(0.0, 0.0)
        };
        put(&mut sim, hero, at);
        sim.set_input(PlayerId(1), InputState::default());
        sim.step();
        for e in sim.fx() {
            match e.kind {
                FxKind::Lob if e.from == g => {
                    lobbed.get_or_insert(e.tick);
                }
                // (One thrown while the fight was set up does not count.)
                FxKind::Bomb if e.to == g && lobbed.is_some() => bursts += 1,
                FxKind::Bomb if e.to == hero && lobbed.is_some() => hurt += e.amount as u32,
                _ => {}
            }
        }
        if let Some(t) = lobbed {
            // Out of the ring before it bursts: only the first bomb counts.
            if sim.tick > t + (BOMB_SECONDS * TICK_HZ as f64) as u64 + 2 {
                break;
            }
        }
    }
    assert!(lobbed.is_some(), "it never threw");
    (bursts, hurt)
}

#[test]
fn a_bomb_bursts_where_the_hero_stood_and_a_step_aside_escapes_it() {
    let (bursts, hurt) = bombed(0.0);
    assert_eq!(bursts, 1);
    assert!(hurt > 0, "standing still, the hero is caught");
    let (bursts, hurt) = bombed(5.0);
    assert_eq!(bursts, 1, "it bursts all the same");
    assert_eq!(hurt, 0, "out of the ring, unhurt");
}

#[test]
fn the_orderly_patches_up_a_hurt_friend() {
    let (mut sim, hero, foes) = street(EnemyKind::Orderly, 2, 12.0);
    // One of them is badly hurt.
    let mut f = sim.fighter(foes[1]).unwrap();
    f.health = f.max_health * 0.3;
    let low = f.health;
    sim.set_fighter(foes[1], f);
    let fx = run(
        &mut sim,
        InputState::default(),
        (ORDERLY_EVERY * 2.0 * TICK_HZ as f64) as u64,
    );
    let patched: Vec<_> = fx
        .iter()
        .filter(|e| e.kind == FxKind::Patch && e.from == foes[0] && e.to == foes[1])
        .collect();
    assert!(!patched.is_empty(), "never mended");
    assert!(patched.iter().all(|e| e.amount > 0));
    assert!(sim.fighter(foes[1]).unwrap().health > low);
    // Not on the hero's side: it never mends a hero.
    assert!(!fx.iter().any(|e| e.kind == FxKind::Patch && e.to == hero));
}
