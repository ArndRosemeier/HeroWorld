//! A power key never just does nothing: a power pressed again while it
//! recharges fires once it is ready, and one that cannot fire tells the
//! hero why (`FxKind::Fizzle`).

use glam::{DVec2, DVec3};
use hw_core::{NetId, Seed};
use hw_net::{fizzle, Fx, FxKind, InputState, PlayerId, Power};
use hw_sim::balance::hero_sheet;
use hw_sim::collision::{Body, Obstacles, Solid};
use hw_sim::{Sim, GROUND_Y, TICK_HZ};
use std::sync::Arc;

fn ground(x: f64, z: f64) -> DVec3 {
    DVec3::new(x, GROUND_Y, z)
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

/// A level 1 hero at the origin and a group `at` metres down the street,
/// with, if `wall`, a wall across the street halfway.
fn street(at: f64, wall: bool) -> (Sim, NetId, Vec<NetId>) {
    let mut solids = Obstacles::flat(GROUND_Y);
    if wall {
        solids.add(Solid {
            origin: DVec2::new(at * 0.5, -30.0),
            along: DVec2::new(0.0, 1.0),
            inward: DVec2::new(1.0, 0.0),
            min: DVec2::new(0.0, 0.0),
            max: DVec2::new(60.0, 0.3),
            y0: GROUND_Y,
            y1: GROUND_Y + 4.0,
        });
    }
    let mut sim = Sim::with_solids(Seed(5), Arc::new(solids));
    sim.set_spawn((DVec2::new(90.0, 90.0), DVec2::X));
    let hero = sim.spawn_hero_with(
        PlayerId(1),
        ground(0.0, 0.0),
        hero_sheet(1, &[Power::Strike, Power::Bolt]),
    );
    sim.step();
    sim.add_fight_spot(ground(at, 0.0), DVec2::new(-1.0, 0.0));
    sim.step();
    let foes: Vec<NetId> = sim.enemies().iter().map(|e| e.0).collect();
    (sim, hero, foes)
}

/// Runs `ticks` ticks, everyone held in place; `press(k)` says whether the
/// bolt key goes down on tick `k`. Returns the hero's events.
fn run(
    sim: &mut Sim,
    hero: NetId,
    foes: &[NetId],
    at: f64,
    ticks: u64,
    press: impl Fn(u64) -> bool,
) -> Vec<Fx> {
    let mut seen = Vec::new();
    for k in 0..ticks {
        put(sim, hero, ground(0.0, 0.0));
        for (n, &f) in foes.iter().enumerate() {
            put(sim, f, ground(at, (n as f64 - 2.0) * 0.8));
        }
        sim.set_input(
            PlayerId(1),
            InputState {
                blast: press(k),
                target: Some(foes[0]),
                aim: [1.0, 0.0, 0.0],
                ..Default::default()
            },
        );
        sim.step();
        seen.extend(sim.fx().iter().copied().filter(|e| e.from == hero));
    }
    seen
}

#[test]
fn pressing_again_while_it_recharges_fires_when_ready() {
    let (mut sim, hero, foes) = street(15.0, false);
    // A second press a fifth of a second after the first: the bolt is still
    // recharging (1.6 s), longer than a key waits otherwise.
    let again = (TICK_HZ as f64 * 0.2) as u64;
    let fx = run(&mut sim, hero, &foes, 15.0, TICK_HZ as u64 * 3, |k| {
        k == 0 || k == again
    });
    let bolts = fx.iter().filter(|e| e.kind == FxKind::Bolt).count();
    assert_eq!(bolts, 2, "{fx:?}");
    assert!(!fx.iter().any(|e| e.kind == FxKind::Fizzle));
}

/// The reason the hero is told when the bolt cannot fire.
fn told(at: f64, wall: bool) -> Vec<u16> {
    let (mut sim, hero, foes) = street(at, wall);
    let fx = run(&mut sim, hero, &foes, at, TICK_HZ as u64 * 2, |k| k == 0);
    assert!(!fx.iter().any(|e| e.kind == FxKind::Bolt), "it fired");
    fx.iter()
        .filter(|e| e.kind == FxKind::Fizzle && e.to == hero)
        .inspect(|e| assert_eq!(e.theme, Power::Bolt.code()))
        .map(|e| e.amount & 0xff)
        .collect()
}

#[test]
fn a_power_that_cannot_fire_says_why() {
    assert_eq!(told(55.0, false), vec![fizzle::OUT_OF_RANGE]);
    assert_eq!(told(15.0, true), vec![fizzle::NOT_SEEN]);
}

#[test]
fn pressing_again_and_again_says_why_each_time() {
    // Behind a wall, the key pressed every half second: the old wait was
    // started over by each press and never lapsed, so nothing was said.
    let (mut sim, hero, foes) = street(15.0, true);
    let half = (TICK_HZ as f64 * 0.5) as u64;
    let fx = run(&mut sim, hero, &foes, 15.0, TICK_HZ as u64 * 4, |k| {
        k % half == 0
    });
    let told = fx
        .iter()
        .filter(|e| e.kind == FxKind::Fizzle && e.amount & 0xff == fizzle::NOT_SEEN)
        .count();
    // Once a second at most: three or four times in four seconds.
    assert!((3..=5).contains(&told), "told {told} times");
}

#[test]
fn holding_the_mouse_button_does_not_swallow_a_power_key() {
    // Out of punching reach with the left button held: a bolt, and a
    // second one asked for while the first recharges, which waits for it
    // (the held button used to push it aside).
    let (mut sim, hero, foes) = street(15.0, false);
    let mut bolts = 0;
    for k in 0..TICK_HZ as u64 * 2 {
        put(&mut sim, hero, ground(0.0, 0.0));
        for (n, &f) in foes.iter().enumerate() {
            put(&mut sim, f, ground(15.0, (n as f64 - 2.0) * 0.8));
        }
        sim.set_input(
            PlayerId(1),
            InputState {
                smash: true,
                blast: k == 0 || k == 10,
                target: Some(foes[0]),
                aim: [1.0, 0.0, 0.0],
                ..Default::default()
            },
        );
        sim.step();
        bolts += sim
            .fx()
            .iter()
            .filter(|e| e.kind == FxKind::Bolt && e.from == hero)
            .count();
    }
    assert_eq!(bolts, 2);
}
