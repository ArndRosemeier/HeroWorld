//! Nobody fights through walls: a hero's punch and bolt, and a villain's
//! blow, need a clear line from chest to chest. With the wall gone, the same
//! blows land (so the tests see the wall, not something else).

use glam::{DVec2, DVec3};
use hw_core::{NetId, Seed};
use hw_net::{FxKind, InputState, PlayerId, Power};
use hw_sim::balance::hero_sheet;
use hw_sim::collision::{Body, Obstacles, Solid};
use hw_sim::combat::{clear_line, in_sight};
use hw_sim::{Sim, GROUND_Y, TICK_HZ};
use std::sync::Arc;

fn ground(x: f64, z: f64) -> DVec3 {
    DVec3::new(x, GROUND_Y, z)
}

/// Flat ground with, if `wall`, a 0.3 m wall across the street at x = 1.
fn town(wall: bool) -> Obstacles {
    let mut solids = Obstacles::flat(GROUND_Y);
    if wall {
        solids.add(Solid {
            origin: DVec2::new(1.0, -30.0),
            along: DVec2::new(0.0, 1.0),
            inward: DVec2::new(1.0, 0.0),
            min: DVec2::new(0.0, 0.0),
            max: DVec2::new(60.0, 0.3),
            y0: GROUND_Y,
            y1: GROUND_Y + 4.0,
        });
    }
    solids
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

/// A hero at the origin and a group of minions, all of them held just
/// behind where the wall is (or would be), 1.9 m away along the street.
fn set_up(wall: bool) -> (Sim, NetId, Vec<NetId>) {
    let mut sim = Sim::with_solids(Seed(9), Arc::new(town(wall)));
    sim.set_spawn((DVec2::new(40.0, 40.0), DVec2::X));
    let hero = sim.spawn_hero_with(
        PlayerId(1),
        ground(0.0, 0.0),
        hero_sheet(10, &[Power::Strike, Power::Bolt]),
    );
    sim.step();
    sim.add_fight_spot(ground(8.0, 0.0), DVec2::new(-1.0, 0.0));
    sim.step();
    let foes: Vec<NetId> = sim.enemies().iter().map(|e| e.0).collect();
    assert!(!foes.is_empty());
    (sim, hero, foes)
}

/// Every villain stands at its place behind the wall, the hero at the origin.
fn hold(sim: &mut Sim, hero: NetId, foes: &[NetId]) {
    put(sim, hero, ground(0.0, 0.0));
    for (k, &f) in foes.iter().enumerate() {
        put(sim, f, ground(1.9, (k as f64 - 2.0) * 0.4));
    }
}

/// Uses `power` on the first villain; returns the events of the next ticks.
fn attack(wall: bool, power: Power) -> (NetId, Vec<hw_net::Fx>) {
    let (mut sim, hero, foes) = set_up(wall);
    let mut seen = Vec::new();
    for k in 0..TICK_HZ as u64 {
        hold(&mut sim, hero, &foes);
        sim.set_input(
            PlayerId(1),
            InputState {
                power: if k == 0 { power.code() } else { 0 },
                target: Some(foes[0]),
                aim: [1.0, 0.0, 0.0],
                ..Default::default()
            },
        );
        sim.step();
        seen.extend(sim.fx().iter().copied().filter(|e| e.to == foes[0]));
    }
    (foes[0], seen)
}

#[test]
fn a_hero_cannot_punch_or_bolt_through_a_wall() {
    for (power, kind) in [(Power::Strike, FxKind::Strike), (Power::Bolt, FxKind::Bolt)] {
        let (_, open) = attack(false, power);
        assert!(
            open.iter().any(|e| e.kind == kind),
            "{power:?} lands with nothing between"
        );
        let (_, walled) = attack(true, power);
        assert!(
            !walled.iter().any(|e| e.kind == kind),
            "{power:?} went through the wall: {walled:?}"
        );
    }
}

/// Blows the villains land on the hero in four seconds, all held in place.
fn blows_on_hero(wall: bool) -> usize {
    let (mut sim, hero, foes) = set_up(wall);
    let mut blows = 0;
    for _ in 0..TICK_HZ as u64 * 4 {
        hold(&mut sim, hero, &foes);
        sim.set_input(PlayerId(1), InputState::default());
        sim.step();
        blows += sim
            .fx()
            .iter()
            .filter(|e| e.to == hero && matches!(e.kind, FxKind::Punch | FxKind::Ray))
            .filter(|e| e.amount > 0)
            .count();
    }
    blows
}

#[test]
fn villains_cannot_hit_a_hero_through_a_wall() {
    assert!(blows_on_hero(false) > 0, "they do hit in the open");
    assert_eq!(blows_on_hero(true), 0, "nothing comes through the wall");
}

#[test]
fn a_thin_inner_wall_blocks_the_line_wherever_it_stands() {
    // A 0.2 m wall (an inner wall) slid along a 1.9 m line in small steps:
    // a line looked at point by point would step over it at some places.
    for k in 0..40 {
        let x = 0.2 + k as f64 * 0.037;
        let mut solids = Obstacles::flat(GROUND_Y);
        solids.add(Solid {
            origin: DVec2::new(x, -5.0),
            along: DVec2::new(0.0, 1.0),
            inward: DVec2::new(1.0, 0.0),
            min: DVec2::new(0.0, 0.0),
            max: DVec2::new(10.0, 0.2),
            y0: GROUND_Y,
            y1: GROUND_Y + 3.0,
        });
        let (a, b) = (ground(0.0, 0.0), ground(1.9, 0.3));
        let up = DVec3::Y * 1.1;
        assert!(!clear_line(&solids, a + up, b + up), "wall at {x} missed");
        // Over the top of it, the line is clear.
        let high = DVec3::Y * 3.5;
        assert!(clear_line(&solids, a + high, b + high));
    }
}

#[test]
fn a_parked_car_hides_the_chest_but_not_the_head() {
    // A car-sized box (1.45 m high) across the line: the chests do not see
    // each other, the heads do, and so does the hero (as the player does
    // over the car's roof). A wall twice as high still hides them.
    let car = |high: f64| {
        let mut solids = Obstacles::flat(GROUND_Y);
        solids.add(Solid {
            origin: DVec2::new(4.0, -2.4),
            along: DVec2::new(0.0, 1.0),
            inward: DVec2::new(1.0, 0.0),
            min: DVec2::new(0.0, 0.0),
            max: DVec2::new(4.8, 1.9),
            y0: GROUND_Y,
            y1: GROUND_Y + high,
        });
        solids
    };
    let chest = DVec3::Y * (hw_sim::collision::BODY_HEIGHT * 0.6);
    let (a, b) = (ground(0.0, 0.0) + chest, ground(12.0, 0.5) + chest);
    let solids = car(1.45);
    assert!(!clear_line(&solids, a, b), "the car is in the chest line");
    assert!(in_sight(&solids, a, b), "seen over the car");
    assert!(in_sight(&solids, b, a), "and back");
    assert!(!in_sight(&car(2.9), a, b), "a wall is a wall");
}
